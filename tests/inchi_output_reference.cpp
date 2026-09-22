// Development-only capture of the pinned InchiToMol C boundary and stages.
// Generated inchi_adapter_body.h retains the original RDKit BSD notice;
// inchi_api.h retains the official InChI notice. This code never enters the app.
#include <GraphMol/PeriodicTable.h>
#include <GraphMol/MolOps.h>
#include <GraphMol/MolPickler.h>
#include <GraphMol/Chirality.h>
#include <GraphMol/Substruct/SubstructMatch.h>
#include <GraphMol/SmilesParse/SmilesParse.h>
#include <RDGeneral/RDLog.h>
#include <inchi_api.h>
#include <algorithm>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <memory>
#include <queue>
#include <set>
#include <sstream>
#include <stack>
#include <string>
#include <tuple>
#include <vector>

namespace observe {
using namespace RDKit;
std::string quoted(const std::string &text) {
  std::ostringstream out;
  out << '"';
  for (unsigned char c : text) {
    if (c == '"' || c == '\\') out << '\\' << c;
    else if (c < 32 || c > 126)
      out << "\\u00" << std::hex << std::setw(2) << std::setfill('0') << int(c);
    else out << c;
  }
  out << '"';
  return out.str();
}
std::string hex(const std::string &bytes) {
  std::ostringstream out;
  for (unsigned char c : bytes)
    out << std::hex << std::setw(2) << std::setfill('0') << int(c);
  return out.str();
}
std::string unhex(const std::string &text) {
  if (text.size() % 2) throw std::runtime_error("Odd pickle encoding");
  std::string out;
  for (size_t i=0; i<text.size(); i+=2)
    out.push_back(char(std::stoul(text.substr(i,2), nullptr,16)));
  return out;
}
std::string state(const ROMol &mol) {
  std::string binary;
  MolPickler::pickleMol(mol,binary,PicklerOps::AllProps);
  const auto *rings=mol.getRingInfo();
  const char *kind=!rings->isInitialized()?"none":rings->isSymmSssr()?"symmetric":rings->isSssrOrBetter()?"basis":"fast";
  std::ostringstream out;
  out << "{\"pickle\":" << quoted(hex(binary)) << ",\"ring_kind\":" << quoted(kind) << ",\"valences\":[";
  for(unsigned i=0;i<mol.getNumAtoms();++i) {
    if(i) out<<',';
    const auto *a=mol.getAtomWithIdx(i);
    try {
      const auto explicit_valence=a->getValence(Atom::ValenceType::EXPLICIT);
      const auto implicit_hydrogens=a->getNumImplicitHs();
      out << "{\"explicit_valence\":" << explicit_valence
          << ",\"implicit_hydrogens\":" << implicit_hydrogens << '}';
    } catch(const std::exception &) { out << "null"; }
  }
  out << "]}";
  return out.str();
}
std::string captured="null";
std::vector<std::pair<std::string,std::string>> stages;
void snapshot(const std::string &name, const ROMol &mol) {
  stages.emplace_back(name,state(mol));
}
bool injected=false;
int injected_status=0;
std::vector<inchi_Atom> atoms;
std::vector<inchi_Stereo0D> stereo;
int capture(inchi_InputINCHI *input, inchi_OutputStruct *output) {
  int status;
  if(injected) {
    *output={};
    output->atom=atoms.data(); output->num_atoms=atoms.size();
    output->stereo0D=stereo.data(); output->num_stereo0D=stereo.size();
    status=injected_status;
  } else status=::GetStructFromINCHI(input,output);
  std::ostringstream out;
  out << std::setprecision(17) << "{\"status\":" << status
      << ",\"message\":" << quoted(output->szMessage?output->szMessage:"")
      << ",\"log\":" << quoted(output->szLog?output->szLog:"")
      << ",\"warning_flags\":[";
  for(int i=0;i<2;++i) {
    if(i) out<<',';
    out<<'['<<output->WarningFlags[i][0]<<','<<output->WarningFlags[i][1]<<']';
  }
  out<<"],\"atoms\":[";
  for(int i=0;i<output->num_atoms;++i) {
    const auto &a=output->atom[i];
    if(i) out<<',';
    out<<"{\"position\":["<<a.x<<','<<a.y<<','<<a.z<<"],\"element\":"<<quoted(a.elname)
       <<",\"isotopic_mass\":"<<a.isotopic_mass<<",\"charge\":"<<int(a.charge)<<",\"hydrogens\":[";
    for(int j=0;j<4;++j) { if(j) out<<',';out<<int(a.num_iso_H[j]); }
    out<<"],\"radical\":"<<int(a.radical)<<",\"bonds\":[";
    for(int j=0;j<a.num_bonds;++j) {
      if(j) out<<',';
      out<<"{\"neighbor\":"<<a.neighbor[j]<<",\"kind\":"<<int(a.bond_type[j])<<",\"stereo\":"<<int(a.bond_stereo[j])<<'}';
    }
    out<<"]}";
  }
  out<<"],\"stereo\":[";
  for(int i=0;i<output->num_stereo0D;++i) {
    const auto &s=output->stereo0D[i];
    if(i) out<<',';
    out<<"{\"central_atom\":";
    if(s.central_atom==NO_ATOM) out<<"null";else out<<s.central_atom;
    out<<",\"neighbors\":[";
    for(int j=0;j<4;++j) {if(j) out<<',';out<<s.neighbor[j];}
    out<<"],\"kind\":"<<int(s.type)<<",\"parity\":"<<int(s.parity)<<'}';
  }
  out<<"]}";
  captured=out.str();
  return status;
}
void release(inchi_OutputStruct *output) { if(!injected) ::FreeStructFromINCHI(output); }
struct ExtraInchiReturnValues {
  int returnCode=0;
  std::string messagePtr,logPtr,auxInfoPtr;
};
#define GetStructFromINCHI capture
#define FreeStructFromINCHI release
#include "inchi_adapter_body.h"
#undef GetStructFromINCHI
#undef FreeStructFromINCHI
} // namespace observe

int main() {
  RDLog::InitLogs();
  std::string line;
  while(std::getline(std::cin,line)) {
    observe::captured="null";
    observe::stages.clear();
    observe::injected=false;
    std::string final="null",error="null";
    try {
      std::istringstream in(line);
      char operation;
      in>>operation;
      if(operation=='C') {
        std::string binary;in>>binary;
        RDKit::RWMol mol;
        RDKit::MolPickler::molFromPickle(observe::unhex(binary),mol);
        observe::snapshot("assembled",mol);
        observe::cleanUp(mol);
        final=observe::state(mol);
      } else {
        int sanitize,remove;
        in>>sanitize>>remove;
        std::string text;
        if(operation=='S') {
          observe::injected=true;
          size_t n,m;in>>observe::injected_status>>n>>m;
          if(n>32767 || m>32767) throw std::runtime_error("Fixture too large");
          observe::atoms.assign(n,{});observe::stereo.assign(m,{});
          for(auto &a:observe::atoms) {
            std::string element;int mass,charge,radical,count;
            in>>element>>mass>>charge>>radical;
            if(element.size()>=sizeof(a.elname)) throw std::runtime_error("Long element");
            std::copy(element.begin(),element.end(),a.elname);
            a.isotopic_mass=mass;a.charge=charge;a.radical=radical;
            for(auto &h:a.num_iso_H) { int value;in>>value;h=value; }
            in>>count;
            if(count<0 || count>MAXVAL) throw std::runtime_error("Invalid fixture degree");
            a.num_bonds=count;
            for(int j=0;j<count;++j) {int other,kind,dir;in>>other>>kind>>dir;a.neighbor[j]=other;a.bond_type[j]=kind;a.bond_stereo[j]=dir;}
          }
          for(auto &s:observe::stereo) {
            int center,kind,parity;in>>center>>kind>>parity;
            s.central_atom=center;s.type=kind;s.parity=parity;
            for(auto &a:s.neighbor) in>>a;
          }
          if(!in) throw std::runtime_error("Truncated fixture");
        } else if(operation=='I') {
          in.get();std::getline(in,text);
        } else throw std::runtime_error("Unknown operation");
        observe::ExtraInchiReturnValues rv;
        std::unique_ptr<RDKit::RWMol> mol(observe::InchiToMol(text,rv,sanitize,remove));
        if(mol) final=observe::state(*mol);
      }
    } catch(const std::exception &e) { error=observe::quoted(e.what()); }
    std::cout<<"{\"raw\":"<<observe::captured<<",\"stages\":{";
    bool first=true;
    for(const auto &[name,state]:observe::stages) {
      if(!first) std::cout<<',';
      first=false;std::cout<<observe::quoted(name)<<':'<<state;
    }
    std::cout<<"},\"final\":"<<final<<",\"error\":"<<error<<'}'<<std::endl;
  }
}
