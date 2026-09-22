// Development-only calls to the pinned native geometry APIs. No Rust code is
// linked and no alignment, rounding or coordinate normalization is performed.
#include <GraphMol/Atom.h>
#include <GraphMol/RWMol.h>
#include <GraphMol/Depictor/DepictUtils.h>
#include <GraphMol/Depictor/EmbeddedFrag.h>
#include <bit>
#include <cstdlib>
#ifdef _WIN32
#include <math.h>
#endif
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

double readDouble(std::istream &input) {
  std::string bits;
  if (!(input >> bits)) throw std::runtime_error("Missing double");
  return std::bit_cast<double>(std::stoull(bits, nullptr, 16));
}
RDGeom::Point2D readPoint(std::istream &input) {
  auto x = readDouble(input);
  auto y = readDouble(input);
  return {x, y};
}
void writeDouble(double value) {
  std::cout << '"' << std::hex << std::setw(16) << std::setfill('0')
            << std::bit_cast<std::uint64_t>(value) << std::dec << '"';
}
void writeResult(const std::vector<int> &ids,
                 const std::vector<double> &values) {
  std::cout << "{\"ids\":[";
  for (std::size_t i = 0; i < ids.size(); ++i) {
    if (i) std::cout << ',';
    std::cout << ids.at(i);
  }
  std::cout << "],\"values\":[";
  for (std::size_t i = 0; i < values.size(); ++i) {
    if (i) std::cout << ',';
    writeDouble(values.at(i));
  }
  std::cout << "]}\n";
}
void writePoints(const RDGeom::INT_POINT2D_MAP &points) {
  std::vector<int> ids;
  std::vector<double> values;
  for (const auto &[id, point] : points) {
    ids.push_back(id);
    values.push_back(point.x);
    values.push_back(point.y);
  }
  writeResult(ids, values);
}
int main() {
  try {
#ifdef _WIN32
    if (const auto* requested = std::getenv("RESHIKI_REFERENCE_FMA3")) {
      const std::string mode(requested);
      if (mode != "0" && mode != "1") throw std::runtime_error("Invalid reference FMA3 profile");
      const auto enabled = mode == "1" ? 1 : 0;
      if (_set_FMA3_enable(enabled) != enabled)
        throw std::runtime_error("Requested reference FMA3 profile is unavailable");
    }
#endif
    std::string line;
    while (std::getline(std::cin, line)) {
      std::istringstream input(line);
      std::string operation;
      std::size_t count;
      if (!(input >> operation >> count) || count > 100000)
        throw std::runtime_error("Invalid request");
      std::vector<int> ids(count);
      for (auto &id : ids) {
        if (!(input >> id) || id < 0 || id > 100000)
          throw std::runtime_error("Invalid atom index");
      }
      if (operation == "ring") {
        RDDepict::BOND_LEN = readDouble(input);
        writePoints(RDDepict::embedRing(ids));
      } else if (operation == "bisect") {
        auto center = readPoint(input);
        auto angle = readDouble(input);
        auto first = readPoint(input);
        auto second = readPoint(input);
        auto point = RDDepict::computeBisectPoint(center, angle, first, second);
        writeResult({}, {point.x, point.y});
      } else if (operation == "reflect") {
        auto point = readPoint(input);
        auto first = readPoint(input);
        auto second = readPoint(input);
        auto result = RDDepict::reflectPoint(point, first, second);
        writeResult({}, {result.x, result.y});
      } else if (operation == "canonical" || operation == "box") {
        RDGeom::INT_POINT2D_MAP points;
        RDKit::RWMol molecule;
        for (auto id : ids) {
          while (molecule.getNumAtoms() <= static_cast<unsigned>(id))
            molecule.addAtom(new RDKit::Atom(6), true, true);
          points[id] = readPoint(input);
        }
        RDDepict::EmbeddedFrag fragment(&molecule, points);
        if (operation == "canonical") {
          fragment.canonicalizeOrientation();
          points.clear();
          for (const auto &[id, atom] : fragment.GetEmbeddedAtoms())
            points[id] = atom.loc;
          writePoints(points);
        } else {
          fragment.computeBox();
          writeResult({}, {fragment.getBoxPx(), fragment.getBoxNx(),
                           fragment.getBoxPy(), fragment.getBoxNy()});
        }
      } else {
        throw std::runtime_error("Unknown operation");
      }
      std::string trailing;
      if (input >> trailing) throw std::runtime_error("Trailing input");
    }
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
