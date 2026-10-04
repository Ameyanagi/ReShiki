#include "geometry.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
void check(bool condition, const char *message) {
  if (!condition) throw std::runtime_error(message);
}

struct Response {
  RshGeometryResponse value{};
  ~Response() { rsh_geometry_free(&value); }
};

struct Molecule {
  std::vector<RshAtom> atoms;
  std::vector<RshBond> bonds;
  RshGeometryRequest request(uint32_t field, uint32_t operation = 0) const {
    RshGeometryRequest r{};
    r.abi_version = 1;
    r.operation = operation;
    r.field = field;
    r.conformers = 3;
    r.seed = 12345;
    r.max_iterations = 1000;
    r.atoms = atoms.data();
    r.atom_count = atoms.size();
    r.bonds = bonds.data();
    r.bond_count = bonds.size();
    return r;
  }
};

RshAtom atom(uint32_t z, uint32_t chiral = 0) {
  RshAtom a{};
  a.atomic_number = z;
  a.chiral_tag = chiral;
  return a;
}

RshBond bond(uint32_t a, uint32_t b, uint32_t order = 1,
             uint32_t stereo = 0, uint32_t refA = UINT32_MAX,
             uint32_t refB = UINT32_MAX) {
  return {a, b, order, order == 12 ? 1u : 0u, stereo, {refA, refB}};
}

void success(const RshGeometryRequest &r, Response &out) {
  if (rsh_geometry_solve(&r, &out.value) != 0) {
    throw std::runtime_error(out.value.error ? out.value.error : "missing native error");
  }
  check(out.value.owner && out.value.coordinates && !out.value.error,
        "success has invalid ownership");
  check(out.value.original_count == r.atom_count && out.value.atom_count >= r.atom_count,
        "original count changed");
  check(out.value.field == r.field, "force field silently changed");
  check(std::isfinite(out.value.energy) && std::isfinite(out.value.initial_energy),
        "invalid energy");
  for (size_t i = r.atom_count; i < out.value.atom_count; ++i) {
    check(out.value.hydrogen_parents &&
              out.value.hydrogen_parents[i - r.atom_count] < r.atom_count,
          "invalid hydrogen parent mapping");
  }
}

void failure(const RshGeometryRequest &r) {
  Response out;
  check(rsh_geometry_solve(&r, &out.value) == 1, "unsupported input was accepted");
  check(out.value.error && *out.value.error, "failure has no diagnostic");
  check(!out.value.coordinates && !out.value.gradient, "failure published coordinates");
  rsh_geometry_free(&out.value);
  check(!out.value.owner && !out.value.error && out.value.atom_count == 0,
        "free did not zero the response");
  rsh_geometry_free(&out.value);
}

std::vector<double> xyz(const Response &r) {
  return {r.value.coordinates, r.value.coordinates + 3 * r.value.atom_count};
}

double evaluate(const Molecule &m, uint32_t field, const std::vector<double> &coords) {
  auto request = m.request(field, 2);
  request.coordinates = coords.data();
  request.coordinate_count = coords.size() / 3;
  Response out;
  success(request, out);
  check(out.value.gradient, "evaluation omitted its gradient");
  return out.value.energy;
}

void fieldsGradientsAndPins() {
  const Molecule ethanol{{atom(6), atom(8), atom(6)},
                         {bond(0, 2), bond(2, 1)}};
  for (uint32_t field = 0; field < 3; ++field) {
    auto request = ethanol.request(field);
    Response generated;
    success(request, generated);
    check(generated.value.atom_count == 9, "ethanol hydrogen count is wrong");
    check(generated.value.energy <= generated.value.initial_energy + 1e-6,
          "minimization increased energy");
    const auto coords = xyz(generated);
    Response repeated;
    success(request, repeated);
    check(std::abs(repeated.value.energy - generated.value.energy) < 1e-8,
          "same-seed generation is not repeatable");

    // Perturb away from the stationary point to test actual analytic forces.
    auto displaced = coords;
    displaced[0] += 0.13;
    request.operation = 2;
    request.coordinates = displaced.data();
    request.coordinate_count = displaced.size() / 3;
    Response evaluated;
    success(request, evaluated);
    for (size_t component = 0; component < displaced.size(); ++component) {
      constexpr double step = 1e-5;
      auto plus = displaced, minus = displaced;
      plus[component] += step;
      minus[component] -= step;
      const double numeric = (evaluate(ethanol, field, plus) -
                              evaluate(ethanol, field, minus)) / (2 * step);
      const double analytic = evaluated.value.gradient[component];
      check(std::abs(numeric - analytic) < 2e-4 * std::max(1.0, std::abs(numeric)),
            "analytic gradient disagrees with finite differences");
    }
    auto transformed = displaced;
    for (size_t i = 0; i < transformed.size(); i += 3) {
      const double x = transformed[i], y = transformed[i + 1];
      transformed[i] = -y + 6.5;
      transformed[i + 1] = x - 2.0;
      transformed[i + 2] += 1.25;
    }
    check(std::abs(evaluate(ethanol, field, transformed) - evaluated.value.energy) < 1e-7,
          "energy is not invariant to rigid rotation/translation");

    const uint32_t fixed[] = {0, 3};  // Original atom and appended hydrogen.
    request.operation = 1;
    request.max_iterations = 15;
    request.fixed_atoms = fixed;
    request.fixed_atom_count = 2;
    Response relaxed;
    success(request, relaxed);
    for (uint32_t index : fixed) {
      for (size_t axis = 0; axis < 3; ++axis) {
        check(relaxed.value.coordinates[3 * index + axis] == displaced[3 * index + axis],
              "fixed-point position changed");
      }
    }
    check(relaxed.value.energy <= relaxed.value.initial_energy + 1e-6,
          "fixed-point relaxation increased energy");
  }
}

void aromaticityAndFieldSelection() {
  Molecule benzene;
  for (uint32_t i = 0; i < 6; ++i) {
    auto carbon = atom(6);
    carbon.aromatic = 1;
    benzene.atoms.push_back(carbon);
    benzene.bonds.push_back(bond(i, (i + 1) % 6, 12));
  }
  for (uint32_t field = 0; field < 3; ++field) {
    Response generated;
    success(benzene.request(field), generated);
    check(generated.value.atom_count == 12, "aromatic hydrogen count is wrong");
    check(generated.value.energy <= generated.value.initial_energy + 1e-6,
          "aromatic minimization increased energy");
  }

  // MMFF lacks boron atom types, while UFF has a boron parameter. A request
  // must return its chosen field's failure rather than silently switch fields.
  const Molecule borane{{atom(5)}, {}};
  for (uint32_t field : {0u, 1u}) {
    auto request = borane.request(field);
    Response out;
    check(rsh_geometry_solve(&request, &out.value) == 1 && out.value.error &&
              std::string(out.value.error).find("MMFF parameters") != std::string::npos,
          "missing MMFF parameters did not produce the field-specific failure");
  }
  Response uff;
  success(borane.request(2), uff);
  check(uff.value.atom_count == 4, "UFF borane hydrogen count is wrong");

  // The variants have different amide out-of-plane parameters. This guards
  // against an incorrect variant spelling silently selecting MMFF94 twice.
  const Molecule acetamide{{atom(6), atom(6), atom(8), atom(7)},
                            {bond(0, 1), bond(1, 2, 2), bond(1, 3)}};
  Response generated;
  success(acetamide.request(0), generated);
  auto distorted = xyz(generated);
  distorted[3 * 3 + 2] += 0.5;
  const double mmff94 = evaluate(acetamide, 0, distorted);
  const double mmff94s = evaluate(acetamide, 1, distorted);
  check(std::abs(mmff94 - mmff94s) > 1e-3,
        "MMFF94 and MMFF94s did not use distinct amide parameters");
}

void stereochemistry() {
  for (uint32_t tag : {1u, 2u}) {
    // Implicit H becomes an appended fourth neighbor, preserving source parity.
    const Molecule tetra{{atom(6, tag), atom(9), atom(17), atom(35)},
                          {bond(0, 1), bond(0, 2), bond(0, 3)}};
    auto request = tetra.request(1);
    Response generated;
    success(request, generated);
    check(generated.value.atom_count == 5 && generated.value.hydrogen_parents[0] == 0,
          "implicit stereochemical hydrogen was not mapped");
    auto coords = xyz(generated);
    // Mirroring inverts chirality; fresh perception must reject it.
    for (size_t i = 2; i < coords.size(); i += 3) coords[i] *= -1;
    request.operation = 2;
    request.coordinates = coords.data();
    request.coordinate_count = coords.size() / 3;
    failure(request);
    for (size_t i = 2; i < coords.size(); i += 3) coords[i] = 0;
    failure(request);
  }
  for (uint32_t stereo : {2u, 3u, 4u, 5u}) {
    const Molecule alkene{{atom(6), atom(6), atom(6), atom(6)},
                          {bond(0, 1), bond(1, 2, 2, stereo, 0, 3), bond(2, 3)}};
    auto request = alkene.request(0);
    Response generated;
    success(request, generated);
    auto coords = xyz(generated);
    request.operation = 2;
    request.coordinates = coords.data();
    request.coordinate_count = coords.size() / 3;
    Response evaluated;
    success(request, evaluated);
    // Rotate only the designated substituent through 180 degrees about the
    // double-bond axis. Its endpoint distance is preserved but E/Z reverses.
    const double axis[] = {coords[6] - coords[3], coords[7] - coords[4],
                           coords[8] - coords[5]};
    const double axis2 = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];
    const double reference[] = {coords[9] - coords[6], coords[10] - coords[7],
                                coords[11] - coords[8]};
    const double projection = (axis[0] * reference[0] + axis[1] * reference[1] +
                               axis[2] * reference[2]) / axis2;
    for (size_t i = 0; i < 3; ++i) {
      coords[9 + i] = coords[6 + i] + 2 * projection * axis[i] - reference[i];
    }
    failure(request);
    // Collapse a designated reference onto its double-bond endpoint.
    std::copy_n(coords.begin() + 3, 3, coords.begin());
    failure(request);
  }
}

void invalidInputs() {
  Molecule molecule{{atom(6), atom(6)}, {bond(0, 1)}};
  auto r = molecule.request(0);
  r.atoms = nullptr;
  failure(r);
  r = molecule.request(0);
  r.atom_count = 513;
  failure(r);
  r = molecule.request(0);
  r.seed = -1;
  failure(r);
  r = molecule.request(0);
  r.field = 3;
  failure(r);
  molecule.atoms[0].radical = 1;
  failure(molecule.request(0));
  molecule.atoms[0] = atom(0);
  failure(molecule.request(0));
  molecule.atoms[0] = atom(26);
  failure(molecule.request(2));
  molecule.atoms[0] = atom(6);
  molecule.bonds[0].order = 17;
  failure(molecule.request(2));
  molecule.bonds.clear();
  failure(molecule.request(0));
  Response out;
  check(rsh_geometry_solve(nullptr, &out.value) == 1 && out.value.error,
        "null request was not guarded");
  check(rsh_geometry_solve(nullptr, nullptr) == 1, "null response was not guarded");
  rsh_geometry_free(nullptr);
}

void rejectedApplyGeometry() {
  const Molecule ethane{{atom(6), atom(6)}, {bond(0, 1)}};
  Response generated;
  success(ethane.request(0), generated);
  auto coords = xyz(generated);
  auto request = ethane.request(0, 2);
  request.coordinates = coords.data();
  request.coordinate_count = coords.size() / 3;
  // A finite coordinate vector with a broken bond must not pass Apply's
  // last evaluation, even when there are no specified stereo descriptors.
  coords[3] += 10;
  for (size_t h = 2; h < generated.value.atom_count; ++h) {
    if (generated.value.hydrogen_parents[h - 2] == 1) coords[3 * h] += 10;
  }
  failure(request);
  coords = xyz(generated);
  request.coordinates = coords.data();
  std::copy_n(coords.begin(), 3, coords.begin() + 3);
  failure(request);
  // A nonbonded appended H overlaps the other carbon without collapsing
  // their covalent bond; the all-pairs overlap guard must reject this too.
  coords = xyz(generated);
  request.coordinates = coords.data();
  const size_t hydrogen = 2;
  const size_t otherCarbon = 1 - generated.value.hydrogen_parents[0];
  std::copy_n(coords.begin() + 3 * otherCarbon, 3, coords.begin() + 3 * hydrogen);
  failure(request);
}
}  // namespace

int main() {
  try {
    fieldsGradientsAndPins();
    aromaticityAndFieldSelection();
    stereochemistry();
    invalidInputs();
    rejectedApplyGeometry();
    std::cout << "Native geometry gradients, invariance, pins, stereo, and ABI guards passed.\n";
    return 0;
  } catch (const std::exception &e) {
    std::cerr << e.what() << '\n';
    return 1;
  }
}
