// Development-only observation of the pinned adapter immediately before GetINCHI.
// Generated inchi_adapter_body.h retains the original RDKit BSD notice; the
// official inchi_api.h retains its InChI MIT notice. No test C++ enters the app.
#include <GraphMol/PeriodicTable.h>
#include <GraphMol/MolOps.h>
#include <GraphMol/MolPickler.h>
#include <GraphMol/Substruct/SubstructMatch.h>
#include <GraphMol/SmilesParse/SmilesParse.h>
#include <RDGeneral/RDLog.h>
#include <inchi_api.h>
#include <algorithm>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace observe {
using namespace RDKit;
std::string captured;
int capture(inchi_Input *input, inchi_Output *output) {
  *output = {};
  std::ostringstream out;
  out << std::setprecision(17) << "{\"atoms\":[";
  for (int i=0; i<input->num_atoms; ++i) {
    const auto &a=input->atom[i];
    if(i) out << ',';
    out << "{\"position\":[" << a.x << ',' << a.y << ',' << a.z
        << "],\"element\":\"" << a.elname << "\",\"isotopic_mass\":" << a.isotopic_mass
        << ",\"charge\":" << int(a.charge) << ",\"hydrogens\":[";
    for(int j=0;j<4;++j) { if(j) out<<','; out<<int(a.num_iso_H[j]); }
    out << "],\"radical\":" << int(a.radical) << ",\"bonds\":[";
    for(int j=0;j<a.num_bonds;++j) {
      if(j) out<<',';
      out << "{\"neighbor\":" << a.neighbor[j] << ",\"kind\":" << int(a.bond_type[j])
          << ",\"stereo\":" << int(a.bond_stereo[j]) << '}';
    }
    out << "]}";
  }
  out << "],\"stereo\":[";
  for(int i=0;i<input->num_stereo0D;++i) {
    const auto &s=input->stereo0D[i];
    if(i) out << ',';
    out << "{\"central_atom\":";
    if(s.central_atom==NO_ATOM) out << "null"; else out << s.central_atom;
    out << ",\"neighbors\":[";
    for(int j=0;j<4;++j) { if(j) out<<','; out<<s.neighbor[j]; }
    out << "],\"kind\":" << int(s.type) << ",\"parity\":" << int(s.parity) << '}';
  }
  out << "]}";
  captured=out.str();
  return 0;
}
void release(inchi_Output *) {}
struct ExtraInchiReturnValues {
  int returnCode=0;
  std::string messagePtr,logPtr,auxInfoPtr;
};
#define GetINCHI capture
#define FreeINCHI release
#include "inchi_adapter_body.h"
#undef GetINCHI
#undef FreeINCHI
}  // namespace observe

int main() {
  RDLog::InitLogs();
  std::string hex;
  while(std::getline(std::cin,hex)) {
    try {
      if(hex.size()%2) throw std::runtime_error("Odd binary encoding");
      std::string binary;
      for(size_t i=0;i<hex.size();i+=2) {
        const auto digit=[](char c) -> unsigned {
          if(c>='0' && c<='9') return c-'0';
          if(c>='a' && c<='f') return c-'a'+10;
          throw std::runtime_error("Invalid binary encoding");
        };
        binary.push_back(char(digit(hex.at(i))*16+digit(hex.at(i+1))));
      }
      RDKit::RWMol mol;
      RDKit::MolPickler::molFromPickle(binary,mol);
      observe::ExtraInchiReturnValues rv;
      observe::captured="null";
      observe::MolToInchi(mol,rv,nullptr);
      std::cout << observe::captured << std::endl;
    } catch(const std::exception &) {
      std::cout << "null" << std::endl;
    }
  }
}
