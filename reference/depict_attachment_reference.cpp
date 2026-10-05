// Direct pinned-native neighbor and non-ring attachment APIs.
// The builder changes the single ` private:` line in EmbeddedFrag.h to
// ` public:` for state observation only; no declaration or ABI is changed.
#include <GraphMol/MolPickler.h>
#include <GraphMol/RWMol.h>
#include <GraphMol/Depictor/DepictUtils.h>
#include <GraphMol/Depictor/EmbeddedFrag.h>
#include <bit>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <memory>
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
  out<<"],\"attachment_points\":[";comma=false;
  for(int id:fragment.d_attachPts){if(comma)out<<',';comma=true;out<<id;}
  out<<"]}";
}
int main() {
  try {
    std::string line;
    while(std::getline(std::cin,line)) {
      std::istringstream input(line);
      std::string pickle,length,mode;
      unsigned seed,count,patch;
      if(!(input>>pickle>>length>>mode>>seed>>count)||count>100000||pickle.size()%2)throw std::runtime_error("Invalid request");
      std::string binary;
      for(std::size_t i=0;i<pickle.size();i+=2)binary.push_back(char(16*digit(pickle.at(i))+digit(pickle.at(i+1))));
      RDKit::RWMol molecule;RDKit::MolPickler::molFromPickle(binary,molecule);
      RDKit::VECT_INT_VECT rings(count);
      for(auto &ring:rings){unsigned n;if(!(input>>n)||n>100000)throw std::runtime_error("Invalid ring");ring.resize(n);for(auto &id:ring)input>>id;}
      input>>patch;
      RDDepict::BOND_LEN=std::bit_cast<double>(std::stoull(length,nullptr,16));
      std::unique_ptr<RDDepict::EmbeddedFrag> value;
      if(mode=="single")value=std::make_unique<RDDepict::EmbeddedFrag>(seed,&molecule);
      else if(mode=="ring")value=std::make_unique<RDDepict::EmbeddedFrag>(&molecule,rings,false);
      else if(mode=="bond")value=std::make_unique<RDDepict::EmbeddedFrag>(molecule.getBondWithIdx(seed));
      else throw std::runtime_error("Invalid seed");
      if(patch){auto &a=value->d_eatoms.at(seed);a.normal=patch==5?RDGeom::Point2D(0.0,0.0):RDGeom::Point2D(0.6,0.8);a.ccw=(patch%2)==0;if(patch>=3)a.CisTransNbr=molecule.getAtomWithIdx(seed)->getDegree()?(*molecule.atomNeighbors(molecule.getAtomWithIdx(seed)).begin())->getIdx():-1;}
      if(patch==4){value->markDone();value->d_px=3.0;value->d_nx=4.0;value->d_py=-5.0;value->d_ny=6.0;auto &a=value->d_eatoms.at(seed);a.df_fixed=true;a.d_density=2.5;}
      RDKit::INT_VECT ids;for(unsigned i=0;i<molecule.getNumAtoms();++i)ids.push_back(i);
      std::cout<<"{\"ascending\":";integers(std::cout,RDDepict::rankAtomsByRank(molecule,ids));
      std::cout<<",\"descending\":";integers(std::cout,RDDepict::rankAtomsByRank(molecule,ids,false));
      std::cout<<",\"initial\":";fragment(std::cout,*value);
      std::cout<<",\"steps\":[";bool comma=false;
      auto record=[&](const std::string &op,unsigned a,unsigned b){if(comma)std::cout<<',';comma=true;std::cout<<"{\"op\":\""<<op<<"\",\"atom\":"<<a<<",\"target\":"<<b<<",\"fragment\":";fragment(std::cout,*value);std::cout<<'}';};
      value->setupNewNeighs();record("setup",0,0);
      for(unsigned step=0;step<molecule.getNumAtoms();++step){
        int target=-1,atom=-1;
        for(int id:value->d_attachPts){const auto &a=value->d_eatoms.at(id);if(!a.neighs.empty()){target=id;atom=a.neighs.front();break;}}
        if(target<0)break;
        try {value->addNonRingAtom(atom,target);} catch(const std::exception &) {
          if(comma)std::cout<<',';comma=true;
          std::cout<<"{\"op\":\"add\",\"atom\":"<<atom<<",\"target\":"<<target<<",\"error\":true,\"fragment\":";
          fragment(std::cout,*value);std::cout<<'}';break;
        }
        record("add",atom,target);
        value->updateNewNeighs(target);record("update",target,0);
        if(step%3==2){value->setupNewNeighs();record("setup",0,0);}
      }
      value->setupNewNeighs();record("setup",0,0);
      std::cout<<"]}\n";
    }
  }catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
