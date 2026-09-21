// Development-only adapter to the pinned RDKit library. No chemistry is
// implemented here: CIPMol supplies its bond orders and resonance fractions.
#include <GraphMol/MolPickler.h>
#include <GraphMol/CIPLabeler/CIPMol.h>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>

int digit(char c) {
  if (c >= '0' && c <= '9') return c - '0';
  if (c >= 'a' && c <= 'f') return c - 'a' + 10;
  throw std::runtime_error("Invalid hexadecimal pickle");
}
int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    try {
      std::istringstream request(line);
      std::string mode, pickle;
      if (!(request >> mode >> pickle)) throw std::runtime_error("Missing request");
      line = pickle;
      if (line.size() % 2) throw std::runtime_error("Odd pickle length");
      std::string binary;
      for (std::size_t i = 0; i < line.size(); i += 2)
        binary.push_back(char(16 * digit(line[i]) + digit(line[i+1])));
      RDKit::RWMol mol;
      RDKit::MolPickler::molFromPickle(binary, mol);
      RDKit::CIPLabeler::CIPMol cip(mol);
      if (mode == "rings") {
        for (unsigned i=0; i<cip.getNumBonds(); ++i)
          cip.isInRing(cip.getBond(i));
      } else if (mode == "fractions") {
        for (unsigned i=0; i<cip.getNumAtoms(); ++i)
          cip.getFractionalAtomicNum(cip.getAtom(i));
      } else if (mode != "bonds") {
        throw std::runtime_error("Unknown access order");
      }
      std::ostringstream result;
      result << "{\"orders\":[";
      for (unsigned i=0; i<cip.getNumBonds(); ++i) {
        if (i) result << ',';
        result << cip.getBondOrder(cip.getBond(i));
      }
      result << "],\"fractions\":[";
      for (unsigned i=0; i<cip.getNumAtoms(); ++i) {
        if (i) result << ',';
        const auto fraction = cip.getFractionalAtomicNum(cip.getAtom(i));
        result << '[' << fraction.numerator() << ',' << fraction.denominator() << ']';
      }
      result << "],\"ring_bonds\":[";
      for (unsigned i=0; i<cip.getNumBonds(); ++i) {
        if (i) result << ',';
        result << (cip.isInRing(cip.getBond(i)) ? "true" : "false");
      }
      result << "]}";
      std::cout << result.str() << std::endl;
    } catch (const std::exception &) { std::cout << "null" << std::endl; }
  }
}
