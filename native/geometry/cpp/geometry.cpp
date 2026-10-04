#include "geometry.h"

#include <ForceField/ForceField.h>
#include <GraphMol/Atom.h>
#include <GraphMol/Bond.h>
#include <GraphMol/Conformer.h>
#include <GraphMol/DistGeomHelpers/Embedder.h>
#include <GraphMol/ForceFieldHelpers/MMFF/AtomTyper.h>
#include <GraphMol/ForceFieldHelpers/MMFF/Builder.h>
#include <GraphMol/ForceFieldHelpers/UFF/AtomTyper.h>
#include <GraphMol/ForceFieldHelpers/UFF/Builder.h>
#include <GraphMol/MolOps.h>
#include <GraphMol/RWMol.h>

#include <algorithm>
#include <cmath>
#include <limits>
#include <memory>
#include <new>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace {
constexpr size_t kMaxOriginalAtoms = 512;
constexpr size_t kMaxCalculationAtoms = 4096;
constexpr size_t kMaxBonds = 2048;
constexpr double kCoordinateLimit = 1e6;
constexpr uint32_t kAbsent = std::numeric_limits<uint32_t>::max();

struct ResultStorage {
  std::vector<double> coordinates;
  std::vector<double> gradient;
  std::vector<uint32_t> hydrogenParents;
  std::string diagnostics;
  std::string error;
};

void require(bool condition, const std::string &message) {
  if (!condition) {
    throw std::invalid_argument(message);
  }
}

std::string atomMessage(size_t index, const char *message) {
  return "atom " + std::to_string(index) + ": " + message;
}

bool supportedElement(uint32_t z) {
  switch (z) {
    case 1: case 5: case 6: case 7: case 8: case 9:
    case 14: case 15: case 16: case 17: case 33: case 34: case 35: case 53:
      return true;
    default:
      return false;
  }
}

void validateRequest(const RshGeometryRequest &r) {
  require(r.abi_version == 1, "unsupported geometry ABI version");
  require(r.operation <= 2, "unsupported geometry operation");
  require(r.field <= 2, "unsupported force field");
  require(r.atom_count > 0 && r.atom_count <= kMaxOriginalAtoms,
          "original atom count must be between 1 and 512");
  require(r.atoms != nullptr, "null atom records");
  require(r.bond_count <= kMaxBonds, "bond count exceeds 2048");
  require(r.bond_count == 0 || r.bonds != nullptr, "null bond records");
  require(r.conformers >= 1 && r.conformers <= 32,
          "conformer count must be between 1 and 32");
  require(r.seed >= 0, "seed must be nonnegative");
  require(r.max_iterations >= 1 && r.max_iterations <= 10000,
          "iteration limit must be between 1 and 10000");
  require(r.coordinate_count <= kMaxCalculationAtoms,
          "coordinate count exceeds 4096");
  require(r.coordinate_count == 0 || r.coordinates != nullptr,
          "null coordinate array");
  require(r.fixed_atom_count <= kMaxCalculationAtoms,
          "fixed atom count exceeds 4096");
  require(r.fixed_atom_count == 0 || r.fixed_atoms != nullptr,
          "null fixed atom array");
  if (r.operation == 0) {
    require(r.coordinate_count == 0 && r.fixed_atom_count == 0,
            "generation does not accept coordinates or fixed atoms");
  } else {
    require(r.coordinate_count > 0, "complete all-atom coordinates required");
  }
  if (r.operation == 2) {
    require(r.fixed_atom_count == 0,
            "energy evaluation does not accept fixed atoms");
  }
  for (size_t i = 0; i < r.coordinate_count * 3; ++i) {
    require(std::isfinite(r.coordinates[i]) &&
                std::abs(r.coordinates[i]) <= kCoordinateLimit,
            "coordinates must be finite and within 1e6 Angstrom");
  }
}

RDKit::RWMol makeMolecule(const RshGeometryRequest &r) {
  RDKit::RWMol mol;
  for (size_t i = 0; i < r.atom_count; ++i) {
    const auto &a = r.atoms[i];
    require(supportedElement(a.atomic_number),
            atomMessage(i, "query, metal, or unsupported element"));
    require(a.radical == 0, atomMessage(i, "radicals are unsupported"));
    require(a.charge >= -8 && a.charge <= 8,
            atomMessage(i, "formal charge is outside supported range"));
    require(a.isotope <= 1000 && a.explicit_h <= 8,
            atomMessage(i, "invalid isotope or hydrogen count"));
    require(a.no_implicit <= 1 && a.aromatic <= 1 && a.chiral_tag <= 2,
            atomMessage(i, "unsupported atom flag or stereo type"));
    RDKit::Atom atom(a.atomic_number);
    atom.setIsotope(a.isotope);
    atom.setFormalCharge(a.charge);
    atom.setNumExplicitHs(a.explicit_h);
    atom.setNoImplicit(a.no_implicit != 0);
    atom.setIsAromatic(a.aromatic != 0);
    atom.setChiralTag(static_cast<RDKit::Atom::ChiralType>(a.chiral_tag));
    atom.setProp<unsigned int>("_rshOriginalIndex", static_cast<unsigned int>(i));
    require(mol.addAtom(&atom) == i, "atom ordering was not preserved");
  }
  std::set<std::pair<uint32_t, uint32_t>> seenBonds;
  for (size_t i = 0; i < r.bond_count; ++i) {
    const auto &b = r.bonds[i];
    const std::string prefix = "bond " + std::to_string(i) + ": ";
    require(b.a < r.atom_count && b.b < r.atom_count && b.a != b.b,
            prefix + "invalid endpoints");
    require(seenBonds.emplace(std::min(b.a, b.b), std::max(b.a, b.b)).second,
            prefix + "duplicate bond");
    require(b.order == 1 || b.order == 2 || b.order == 3 || b.order == 12,
            prefix + "query or coordination bond is unsupported");
    require(b.aromatic <= 1 && b.stereo <= 5,
            prefix + "unsupported bond flag or stereo type");
    require(b.stereo <= 1 || b.order == 2,
            prefix + "specified stereo requires a double bond");
    require((b.stereo_atoms[0] == kAbsent) ==
                (b.stereo_atoms[1] == kAbsent),
            prefix + "incomplete stereo references");
    require(b.stereo <= 1 || b.stereo_atoms[0] != kAbsent,
            prefix + "specified stereo requires reference atoms");
    mol.addBond(b.a, b.b, static_cast<RDKit::Bond::BondType>(b.order));
    auto *bond = mol.getBondWithIdx(static_cast<unsigned int>(i));
    bond->setIsAromatic(b.aromatic != 0 || b.order == 12);
  }
  // All bonds must exist before RDKit validates stereo reference adjacency.
  for (size_t i = 0; i < r.bond_count; ++i) {
    const auto &b = r.bonds[i];
    auto *bond = mol.getBondWithIdx(static_cast<unsigned int>(i));
    if (b.stereo_atoms[0] != kAbsent) {
      require(b.stereo_atoms[0] < r.atom_count &&
                  b.stereo_atoms[1] < r.atom_count &&
                  b.stereo_atoms[0] != b.stereo_atoms[1] &&
                  b.stereo_atoms[0] != b.b && b.stereo_atoms[1] != b.a &&
                  mol.getBondBetweenAtoms(b.a, b.stereo_atoms[0]) != nullptr &&
                  mol.getBondBetweenAtoms(b.b, b.stereo_atoms[1]) != nullptr,
              "invalid double-bond stereo reference adjacency");
      bond->setStereoAtoms(b.stereo_atoms[0], b.stereo_atoms[1]);
    }
    bond->setStereo(static_cast<RDKit::Bond::BondStereo>(b.stereo));
  }
  RDKit::MolOps::sanitizeMol(mol);
  std::vector<int> fragments;
  require(RDKit::MolOps::getMolFrags(mol, fragments) == 1,
          "geometry requests must contain one connected covalent component");
  for (auto atom : mol.atoms()) {
    require(atom->getNumRadicalElectrons() == 0,
            atomMessage(atom->getIdx(), "sanitization produced a radical"));
  }
  RDKit::MolOps::addHs(mol, false, false);
  require(mol.getNumAtoms() <= kMaxCalculationAtoms,
          "hydrogen-expanded atom count exceeds 4096");
  for (size_t i = 0; i < r.atom_count; ++i) {
    require(mol.getAtomWithIdx(i)->getProp<unsigned int>("_rshOriginalIndex") == i,
            "hydrogen expansion changed original atom ordering");
  }
  for (size_t i = r.atom_count; i < mol.getNumAtoms(); ++i) {
    require(mol.getAtomWithIdx(i)->getAtomicNum() == 1,
            "hydrogen expansion appended a non-hydrogen atom");
  }
  return mol;
}

struct StereoExpectation {
  std::vector<std::pair<unsigned int, RDKit::Atom::ChiralType>> atoms;
  struct DoubleBond {
    unsigned int index, a, b, refA, refB;
    bool sameSide;
  };
  std::vector<DoubleBond> bonds;
};

StereoExpectation stereoExpectation(const RDKit::RWMol &mol) {
  StereoExpectation result;
  for (auto a : mol.atoms()) {
    if (a->getChiralTag() != RDKit::Atom::CHI_UNSPECIFIED) {
      require(a->getDegree() >= 3 && a->getDegree() <= 4,
              atomMessage(a->getIdx(), "invalid tetrahedral stereo degree"));
      result.atoms.emplace_back(a->getIdx(), a->getChiralTag());
    }
  }
  for (auto b : mol.bonds()) {
    const auto stereo = b->getStereo();
    if (stereo >= RDKit::Bond::STEREOZ && stereo <= RDKit::Bond::STEREOTRANS) {
      const auto &refs = b->getStereoAtoms();
      require(refs.size() == 2, "specified double bond has no stereo references");
      result.bonds.push_back({b->getIdx(), b->getBeginAtomIdx(),
                              b->getEndAtomIdx(), static_cast<unsigned>(refs[0]),
                              static_cast<unsigned>(refs[1]),
                              stereo == RDKit::Bond::STEREOZ ||
                                  stereo == RDKit::Bond::STEREOCIS});
    }
  }
  return result;
}

void validateCoordinates(const RDKit::Conformer &conf) {
  for (unsigned int i = 0; i < conf.getNumAtoms(); ++i) {
    const auto &p = conf.getAtomPos(i);
    require(std::isfinite(p.x) && std::isfinite(p.y) && std::isfinite(p.z) &&
                std::abs(p.x) <= kCoordinateLimit &&
                std::abs(p.y) <= kCoordinateLimit &&
                std::abs(p.z) <= kCoordinateLimit,
            "force field produced invalid coordinates");
  }
}

void validateChemicalGeometry(const RDKit::RWMol &mol, int conformer) {
  const auto &conf = mol.getConformer(conformer);
  validateCoordinates(conf);
  // These broad bounds reject collapsed particles and broken covalent bonds;
  // they are acceptance guards, not an alternative force-field model.
  for (auto bond : mol.bonds()) {
    const double distance2 = (conf.getAtomPos(bond->getBeginAtomIdx()) -
                              conf.getAtomPos(bond->getEndAtomIdx())).lengthSq();
    require(distance2 >= 0.04 && distance2 <= 16.0,
            "optimized covalent bond is collapsed or exceeds 4 Angstrom");
  }
  for (unsigned int a = 0; a < mol.getNumAtoms(); ++a) {
    for (unsigned int b = a + 1; b < mol.getNumAtoms(); ++b) {
      require((conf.getAtomPos(a) - conf.getAtomPos(b)).lengthSq() >= 0.04,
              "optimized atoms overlap within 0.2 Angstrom");
    }
  }
}

void validateStereo(const RDKit::RWMol &mol, int conformer,
                    const StereoExpectation &expected) {
  const auto &conf = mol.getConformer(conformer);
  validateCoordinates(conf);
  // A separate copy is essential: fresh perception must never turn an
  // unspecified source center into a document stereo descriptor.
  RDKit::RWMol check(mol);
  RDKit::MolOps::removeStereochemistry(check);
  RDKit::MolOps::assignStereochemistryFrom3D(check, conformer, true);
  for (const auto &[index, tag] : expected.atoms) {
    require(check.getAtomWithIdx(index)->getChiralTag() == tag,
            atomMessage(index, "specified tetrahedral stereo inverted or degenerated"));
  }
  for (const auto &b : expected.bonds) {
    require(check.getBondWithIdx(b.index)->getStereo() > RDKit::Bond::STEREOANY,
            "specified double-bond stereo degenerated");
    const auto axis = conf.getAtomPos(b.b) - conf.getAtomPos(b.a);
    const double axis2 = axis.lengthSq();
    require(axis2 > 1e-8, "specified double-bond endpoints coincide");
    auto u = conf.getAtomPos(b.refA) - conf.getAtomPos(b.a);
    auto v = conf.getAtomPos(b.refB) - conf.getAtomPos(b.b);
    u -= axis * (u.dotProduct(axis) / axis2);
    v -= axis * (v.dotProduct(axis) / axis2);
    const double norm2 = u.lengthSq() * v.lengthSq();
    require(norm2 > 1e-12, "specified double-bond stereo references are collinear");
    const double cosine = u.dotProduct(v) / std::sqrt(norm2);
    require(std::abs(cosine) >= 0.8 && ((cosine > 0) == b.sameSide),
            "specified double-bond stereo inverted or degenerated");
  }
}

class FieldBuilder {
 public:
  FieldBuilder(RDKit::RWMol &mol, uint32_t field) : mol_(mol), field_(field) {
    if (field_ == 2) {
      auto atomTypes = RDKit::UFF::getAtomTypes(mol_);
      require(atomTypes.second &&
                  std::all_of(atomTypes.first.begin(), atomTypes.first.end(),
                              [](const auto *p) { return p != nullptr; }),
              "UFF parameters are unavailable for this molecule");
      uff_ = std::move(atomTypes.first);
    } else {
      mmff_ = std::make_unique<RDKit::MMFF::MMFFMolProperties>(
          mol_, field_ == 0 ? "MMFF94" : "MMFF94s");
      require(mmff_->isValid(),
              "MMFF parameters are unavailable for this molecule");
    }
  }

  std::unique_ptr<ForceFields::ForceField> build(int conformer) {
    // Include every nonbonded pair. In particular, dragging must not retain
    // a neighbor list pruned using coordinates before the target moved.
    const double cutoff = std::numeric_limits<double>::infinity();
    std::unique_ptr<ForceFields::ForceField> ff(
        field_ == 2 ? RDKit::UFF::constructForceField(mol_, uff_, cutoff, conformer, true)
                    : RDKit::MMFF::constructForceField(mol_, mmff_.get(), cutoff,
                                                      conformer, true));
    require(ff != nullptr, "force-field construction failed");
    ff->initialize();
    require(ff->dimension() == 3 && ff->numPoints() == mol_.getNumAtoms(),
            "force-field coordinate dimensions do not match the molecule");
    return ff;
  }

 private:
  RDKit::RWMol &mol_;
  uint32_t field_;
  std::unique_ptr<RDKit::MMFF::MMFFMolProperties> mmff_;
  RDKit::UFF::AtomicParamVect uff_;
};

std::vector<double> coordinates(const RDKit::Conformer &conf) {
  std::vector<double> result;
  result.reserve(3 * conf.getNumAtoms());
  for (unsigned int i = 0; i < conf.getNumAtoms(); ++i) {
    const auto &p = conf.getAtomPos(i);
    result.insert(result.end(), {p.x, p.y, p.z});
  }
  return result;
}

void installCoordinates(RDKit::RWMol &mol, const RshGeometryRequest &r) {
  require(r.coordinate_count == mol.getNumAtoms(),
          "coordinate count must include every original and calculation hydrogen");
  auto conf = std::make_unique<RDKit::Conformer>(mol.getNumAtoms());
  conf->set3D(true);
  for (unsigned int i = 0; i < mol.getNumAtoms(); ++i) {
    conf->setAtomPos(i, RDGeom::Point3D(r.coordinates[3 * i],
                                     r.coordinates[3 * i + 1],
                                     r.coordinates[3 * i + 2]));
  }
  mol.addConformer(conf.release(), true);
}

double energy(ForceFields::ForceField &ff) {
  const double e = ff.calcEnergy();
  require(std::isfinite(e), "force field produced a nonfinite energy");
  return e;
}

void publish(std::unique_ptr<ResultStorage> storage, RshGeometryResponse &out) {
  out.coordinates = storage->coordinates.empty() ? nullptr : storage->coordinates.data();
  out.gradient = storage->gradient.empty() ? nullptr : storage->gradient.data();
  out.hydrogen_parents = storage->hydrogenParents.empty()
                            ? nullptr : storage->hydrogenParents.data();
  out.diagnostics = storage->diagnostics.c_str();
  out.error = storage->error.empty() ? nullptr : storage->error.c_str();
  out.owner = storage.release();
}

void solve(const RshGeometryRequest &r, RshGeometryResponse &out) {
  validateRequest(r);
  auto mol = makeMolecule(r);
  const auto expected = stereoExpectation(mol);
  FieldBuilder builder(mol, r.field);
  auto storage = std::make_unique<ResultStorage>();
  for (size_t i = r.atom_count; i < mol.getNumAtoms(); ++i) {
    auto *hydrogen = mol.getAtomWithIdx(static_cast<unsigned int>(i));
    require(hydrogen->getDegree() == 1,
            "calculation hydrogen must have exactly one original parent");
    const auto neighbor = *mol.atomNeighbors(hydrogen).begin();
    require(neighbor->getIdx() < r.atom_count,
            "calculation hydrogen parent is not an original atom");
    storage->hydrogenParents.push_back(neighbor->getIdx());
  }
  out.field = r.field;
  out.original_count = r.atom_count;
  out.atom_count = mol.getNumAtoms();
  if (r.operation == 0) {
    auto params = RDKit::DGeomHelpers::ETKDGv3;
    params.randomSeed = r.seed;
    params.numThreads = 1;
    params.maxIterations = 100;
    params.timeout = 10;
    params.enforceChirality = true;
    params.pruneRmsThresh = 0.1;
    params.clearConfs = true;
    params.enableSequentialRandomSeeds = true;
    params.trackFailures = true;
    const auto conformers = RDKit::DGeomHelpers::EmbedMultipleConfs(mol, r.conformers, params);
    std::ostringstream failures;
    for (size_t i = 0; i < params.failures.size(); ++i) {
      if (params.failures[i] != 0) {
        failures << " " << i << ":" << params.failures[i];
      }
    }
    require(!conformers.empty(), "ETKDGv3 could not generate a conformer; "
            "embedding failure counters=" + failures.str());
    double best = std::numeric_limits<double>::infinity();
    size_t valid = 0, rejected = 0;
    std::string lastFailure;
    for (int id : conformers) {
      try {
        validateStereo(mol, id, expected);
        auto ff = builder.build(id);
        const double initial = energy(*ff);
        const int status = ff->minimize(r.max_iterations, 1e-4, 1e-6);
        require(status == 0 || status == 1, "unexpected force-field optimizer status");
        const double final = energy(*ff);
        validateChemicalGeometry(mol, id);
        validateStereo(mol, id, expected);
        ++valid;
        if (final < best) {
          best = final;
          storage->coordinates = coordinates(mol.getConformer(id));
          out.initial_energy = initial;
          out.energy = final;
          out.converged = status == 0;
        }
      } catch (const std::exception &e) {
        ++rejected;
        lastFailure = e.what();
      }
    }
    require(valid > 0, "all generated conformers failed validation: " + lastFailure);
    std::ostringstream info;
    info << "ETKDGv3; requested=" << r.conformers
         << "; embedded=" << conformers.size() << "; valid=" << valid
         << "; rejected=" << rejected << "; seed=" << r.seed
         << "; selection=lowest energy among valid generated conformers"
         << "; iteration_limit=" << r.max_iterations
         << "; converged=" << out.converged
         << "; embedding_failure_counters=" << failures.str();
    if (!lastFailure.empty()) {
      info << "; last_rejection=" << lastFailure.substr(0, 512);
    }
    storage->diagnostics = info.str();
  } else {
    installCoordinates(mol, r);
    validateStereo(mol, 0, expected);
    auto ff = builder.build(0);
    out.initial_energy = energy(*ff);
    if (r.operation == 1) {
      std::set<uint32_t> fixed;
      for (size_t i = 0; i < r.fixed_atom_count; ++i) {
        const uint32_t index = r.fixed_atoms[i];
        require(index < mol.getNumAtoms(), "fixed atom index out of range");
        require(fixed.insert(index).second, "duplicate fixed atom index");
        ff->fixedPoints().push_back(static_cast<int>(index));
      }
      ff->initialize();
      const int status = ff->minimize(r.max_iterations, 1e-4, 1e-6);
      require(status == 0 || status == 1, "unexpected force-field optimizer status");
      out.converged = status == 0;
      validateChemicalGeometry(mol, 0);
      validateStereo(mol, 0, expected);
      for (uint32_t index : fixed) {
        const auto &p = mol.getConformer().getAtomPos(index);
        require(p.x == r.coordinates[3 * index] &&
                    p.y == r.coordinates[3 * index + 1] &&
                    p.z == r.coordinates[3 * index + 2],
                "force field changed a fixed atom coordinate");
      }
      storage->diagnostics = "bounded relaxation; fixed_atoms=" +
          std::to_string(fixed.size()) + "; iteration_limit=" +
          std::to_string(r.max_iterations) + "; converged=" +
          std::to_string(out.converged);
    } else {
      // Evaluate is also the final acceptance check before applying a preview.
      // Finite energies alone must not authorize stretched or collapsed bonds.
      validateChemicalGeometry(mol, 0);
      storage->gradient.assign(3 * mol.getNumAtoms(), 0.0);
      ff->calcGrad(storage->gradient.data());
      require(std::all_of(storage->gradient.begin(), storage->gradient.end(),
                          [](double v) { return std::isfinite(v); }),
              "force field produced a nonfinite gradient");
      out.converged = 0;  // Evaluate performs no minimization.
      storage->diagnostics = "energy and analytic gradient; no minimization";
    }
    out.energy = energy(*ff);
    storage->coordinates = coordinates(mol.getConformer());
  }
  publish(std::move(storage), out);
}

void publishError(RshGeometryResponse &out, const char *message) noexcept {
  out = {};
  try {
    auto storage = std::make_unique<ResultStorage>();
    storage->error = message != nullptr ? std::string(message).substr(0, 4096)
                                       : "unknown native geometry failure";
    publish(std::move(storage), out);
  } catch (...) {
    // A static string also makes allocation failures safe across the C ABI.
    out.error = "native geometry failed while allocating error storage";
  }
}
}  // namespace

extern "C" int32_t rsh_geometry_solve(const RshGeometryRequest *request,
                                      RshGeometryResponse *response) noexcept {
  if (response == nullptr) {
    return 1;
  }
  // Never silently leak an earlier result. The zero-initialization requirement
  // is explicit in the ABI; a caller can still free the previous response.
  if (response->owner != nullptr) {
    return 1;
  }
  *response = {};
  try {
    require(request != nullptr, "null geometry request");
    solve(*request, *response);
    return 0;
  } catch (const std::exception &e) {
    publishError(*response, e.what());
  } catch (...) {
    publishError(*response, "unknown native geometry exception");
  }
  return 1;
}

extern "C" void rsh_geometry_free(RshGeometryResponse *response) noexcept {
  if (response != nullptr) {
    delete static_cast<ResultStorage *>(response->owner);
    *response = {};
  }
}
