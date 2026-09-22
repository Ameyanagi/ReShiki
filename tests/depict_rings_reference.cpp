// Direct pinned-native ring-selection and no-template EmbeddedFrag APIs.
#include <GraphMol/MolPickler.h>
#include <GraphMol/RWMol.h>
#include <GraphMol/Depictor/DepictUtils.h>
#include <GraphMol/Depictor/EmbeddedFrag.h>
#include <bit>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>

int digit(char c) {
  if (c >= '0' && c <= '9') return c-'0';
  if (c >= 'a' && c <= 'f') return c-'a'+10;
  throw std::runtime_error("Invalid pickle digit");
}
void integers(std::ostream &out,const RDKit::INT_VECT &values) {
  out << '[';
  for(std::size_t i=0;i<values.size();++i){if(i)out<<',';out<<values.at(i);}
  out << ']';
}
void number(std::ostream &out,double value) {
  out << '"' << std::hex << std::setw(16) << std::setfill('0') << std::bit_cast<std::uint64_t>(value) << std::dec << '"';
}
void fragment(std::ostream &out,RDDepict::EmbeddedFrag &fragment) {
  out<<"{\"done\":"<<(fragment.isDone()?"true":"false")<<",\"bounds\":[";
  bool comma=false;
  for(double value:{fragment.getBoxPx(),fragment.getBoxNx(),fragment.getBoxPy(),fragment.getBoxNy()}){if(comma)out<<',';comma=true;number(out,value);}
  out<<"],\"atoms\":[";comma=false;
  for(const auto &[id,atom]:fragment.GetEmbeddedAtoms()) {
    if(comma)out<<',';comma=true;
    out<<"{\"ints\":["<<id<<','<<atom.aid<<','<<atom.nbr1<<','<<atom.nbr2<<','<<atom.CisTransNbr<<','<<atom.ccw<<','<<atom.rotDir<<','<<atom.df_fixed<<"],\"values\":[";
    bool separator=false;
    for(double value:{atom.loc.x,atom.loc.y,atom.normal.x,atom.normal.y,atom.angle,atom.d_density}){if(separator)out<<',';separator=true;number(out,value);}
    out<<"],\"neighbors\":";integers(out,atom.neighs);out<<'}';
  }
  out<<"]}";
}
int main() {
  try {
    std::string line;
    while(std::getline(std::cin,line)) {
      std::istringstream input(line);
      std::string pickle,length;
      unsigned construct,count;
      if(!(input>>pickle>>construct>>length>>count) || count>100000 || pickle.size()%2)throw std::runtime_error("Invalid request");
      std::string binary;
      for(std::size_t i=0;i<pickle.size();i+=2)binary.push_back(char(16*digit(pickle.at(i))+digit(pickle.at(i+1))));
      RDKit::RWMol molecule;RDKit::MolPickler::molFromPickle(binary,molecule);
      RDKit::VECT_INT_VECT rings(count);
      for(auto &ring:rings) {
        unsigned size;if(!(input>>size)||size>100000)throw std::runtime_error("Invalid ring");
        ring.resize(size);for(auto &id:ring)if(!(input>>id))throw std::runtime_error("Missing atom");
      }
      unsigned size;if(!(input>>size)||size>100000)throw std::runtime_error("Invalid completed rings");
      RDKit::INT_VECT done(size);for(auto &id:done)if(!(input>>id))throw std::runtime_error("Missing completed ring");
      std::string trailing;if(input>>trailing)throw std::runtime_error("Trailing input");
      RDDepict::BOND_LEN=std::bit_cast<double>(std::stoull(length,nullptr,16));
      std::ostringstream result;
      result<<"{\"first\":"<<RDDepict::pickFirstRingToEmbed(molecule,rings)<<",\"core\":";
      RDKit::INT_VECT core;RDDepict::findCoreRings(rings,core,molecule);integers(result,core);
      result<<",\"next\":";
      try {
        int next=-1;auto common=RDDepict::findNextRingToEmbed(done,rings,next);
        result<<"{\"ring\":"<<next<<",\"common_atoms\":";integers(result,common);result<<'}';
      }catch(const std::exception &){result<<"null";}
      result<<",\"fragment\":";
      if(construct) {
        try{RDDepict::EmbeddedFrag value(&molecule,rings,false);fragment(result,value);}
        catch(const std::exception &){result<<"null";}
      }else{result<<"null";}
      result<<'}';std::cout<<result.str()<<'\n';
    }
  }catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
