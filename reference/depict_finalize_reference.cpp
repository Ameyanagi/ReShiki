// Direct pinned-native final-stage observations. The generated header exposes
// private state with the previously documented one-line visibility change only.
#include <GraphMol/Atom.h>
#include <GraphMol/RWMol.h>
#include <GraphMol/Conformer.h>
#include <GraphMol/Depictor/EmbeddedFrag.h>
#include <bit>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <list>
#include <sstream>
#include <stdexcept>
#include <string>
namespace RDDepict {
unsigned int copyCoordinate(RDKit::ROMol &, std::list<EmbeddedFrag> &, bool);
namespace DepictorLocal { void _shiftCoords(std::list<EmbeddedFrag> &); }
}
void singleCoordinate(RDKit::ROMol &, unsigned int, const RDGeom::INT_POINT2D_MAP *);
double readNumber(std::istream &in) {
  std::string bits;if(!(in>>bits))throw std::runtime_error("Missing number");
  return std::bit_cast<double>(std::stoull(bits,nullptr,16));
}
void number(double value) {
  std::cout<<'"'<<std::hex<<std::setw(16)<<std::setfill('0')
           <<std::bit_cast<std::uint64_t>(value)<<std::dec<<'"';
}
template <class Values> void integers(const Values &values) {
  std::cout<<'[';bool comma=false;
  for(auto v:values){if(comma)std::cout<<',';comma=true;std::cout<<v;}
  std::cout<<']';
}
void fragments(std::list<RDDepict::EmbeddedFrag> &values) {
  std::cout<<'[';bool separator=false;
  for(auto &f:values){
    if(separator)std::cout<<',';separator=true;
    std::cout<<"{\"done\":"<<(f.isDone()?"true":"false")<<",\"bounds\":[";
    bool comma=false;
    for(double v:{f.getBoxPx(),f.getBoxNx(),f.getBoxPy(),f.getBoxNy()}){
      if(comma)std::cout<<',';comma=true;number(v);
    }
    std::cout<<"],\"atoms\":[";comma=false;
    for(const auto &[id,a]:f.GetEmbeddedAtoms()){
      if(comma)std::cout<<',';comma=true;
      std::cout<<"{\"ints\":["<<id<<','<<a.aid<<','<<a.nbr1<<','<<a.nbr2<<','
               <<a.CisTransNbr<<','<<a.ccw<<','<<a.rotDir<<','<<a.df_fixed<<"],\"values\":[";
      bool c=false;
      for(double v:{a.loc.x,a.loc.y,a.normal.x,a.normal.y,a.angle,a.d_density}){
        if(c)std::cout<<',';c=true;number(v);
      }
      std::cout<<"],\"neighbors\":";integers(a.neighs);std::cout<<'}';
    }
    std::cout<<"],\"attachment_points\":";integers(f.d_attachPts);std::cout<<'}';
  }
  std::cout<<']';
}
int main(){
 try{
  std::string line;
  while(std::getline(std::cin,line)){
   std::istringstream in(line);unsigned count,canonical,existing,nfrag,ncoord;int hasMap;
   if(!(in>>count>>canonical>>existing>>nfrag>>hasMap>>ncoord)||count>100000||nfrag>100000||ncoord>count)throw std::runtime_error("Invalid request");
   RDKit::RWMol mol;for(unsigned i=0;i<count;++i)mol.addAtom(new RDKit::Atom(6),true,true);
   for(unsigned i=0;i<existing;++i){auto *c=new RDKit::Conformer(count);c->setId(17+i);c->set3D(true);mol.addConformer(c);}
   RDGeom::INT_POINT2D_MAP coordinates;
   for(unsigned i=0;i<ncoord;++i){int id;in>>id;auto x=readNumber(in),y=readNumber(in);coordinates[id]={x,y};}
   const auto *coordMap=hasMap?&coordinates:nullptr;
   std::list<RDDepict::EmbeddedFrag> values;
   for(unsigned i=0;i<nfrag;++i){
    RDGeom::INT_POINT2D_MAP empty;values.emplace_back(&mol,empty);auto &f=values.back();
    int done;in>>done;if(done)f.markDone();
    f.d_px=readNumber(in);f.d_nx=readNumber(in);f.d_py=readNumber(in);f.d_ny=readNumber(in);
    unsigned n;in>>n;
    for(unsigned j=0;j<n;++j){
     int id,ccw,fixed;RDDepict::EmbeddedAtom a;
     in>>id>>a.aid>>a.nbr1>>a.nbr2>>a.CisTransNbr>>ccw>>a.rotDir>>fixed;
     a.ccw=ccw;a.df_fixed=fixed;a.loc.x=readNumber(in);a.loc.y=readNumber(in);
     a.normal.x=readNumber(in);a.normal.y=readNumber(in);a.angle=readNumber(in);a.d_density=readNumber(in);
     unsigned neighbors;in>>neighbors;a.neighs.resize(neighbors);for(auto &v:a.neighs)in>>v;
     f.d_eatoms[id]=a;
    }
    unsigned attached;in>>attached;f.d_attachPts.resize(attached);for(auto &v:f.d_attachPts)in>>v;
   }
   std::string trailing;if(in>>trailing)throw std::runtime_error("Trailing input");
   std::cout<<"{\"initial\":";fragments(values);
   if((!coordMap||coordMap->empty())&&canonical){for(auto &f:values)f.canonicalizeOrientation();}
   std::cout<<",\"canonical\":";fragments(values);
   RDDepict::DepictorLocal::_shiftCoords(values);
   std::cout<<",\"packed\":";fragments(values);
   auto cid=RDDepict::copyCoordinate(mol,values,true);
   singleCoordinate(mol,cid,coordMap);
   const auto &conf=mol.getConformer(cid);
   std::cout<<",\"conformer_id\":"<<cid<<",\"conformer_count\":"<<mol.getNumConformers()
            <<",\"is_3d\":"<<(conf.is3D()?"true":"false")<<",\"positions\":[";
   for(unsigned i=0;i<conf.getNumAtoms();++i){if(i)std::cout<<',';auto p=conf.getAtomPos(i);std::cout<<'[';number(p.x);std::cout<<',';number(p.y);std::cout<<',';number(p.z);std::cout<<']';}
   std::cout<<"]}\n";
  }
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}
}
