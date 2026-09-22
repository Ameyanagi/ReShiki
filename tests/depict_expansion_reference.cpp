// Direct pinned computeInitialCoords and native fragment merging observations.
#include "depict-native-fragment-observation.inc"
#include <GraphMol/MolOps.h>
#include <GraphMol/Conformer.h>
#include <GraphMol/Depictor/RDDepictor.h>
#include <list>
namespace RDDepict {
void computeInitialCoords(RDKit::ROMol&,const RDGeom::INT_POINT2D_MAP*,std::list<EmbeddedFrag>&,bool);
namespace DepictorLocal {
void embedFusedSystems(const RDKit::ROMol&,const RDKit::VECT_INT_VECT&,std::list<EmbeddedFrag>&,const RDGeom::INT_POINT2D_MAP*,bool);
void embedNontetrahedralStereo(const RDKit::ROMol&,std::list<EmbeddedFrag>&,const std::vector<int>&);
void embedCisTransSystems(const RDKit::ROMol&,std::list<EmbeddedFrag>&);
RDKit::INT_LIST getNonEmbeddedAtoms(const RDKit::ROMol&,const std::list<EmbeddedFrag>&);
std::list<EmbeddedFrag>::iterator _findLargestFrag(std::list<EmbeddedFrag>&);
void embedSquarePlanar(const RDKit::ROMol&,const RDKit::Atom*,std::list<EmbeddedFrag>&,const std::vector<int>&);
void embedTBP(const RDKit::ROMol&,const RDKit::Atom*,std::list<EmbeddedFrag>&,const std::vector<int>&);
void embedOctahedral(const RDKit::ROMol&,const RDKit::Atom*,std::list<EmbeddedFrag>&,const std::vector<int>&);
}}
using Fragments=std::list<RDDepict::EmbeddedFrag>;
std::string fragments(Fragments &values) {
  std::ostringstream out;out<<'[';bool comma=false;
  for(auto &f:values){if(comma)out<<',';comma=true;fragment(out,f);}out<<']';return out.str();
}
std::string ids(const RDKit::INT_LIST &values) {
  std::ostringstream out;out<<'[';bool comma=false;for(auto id:values){if(comma)out<<',';comma=true;out<<id;}out<<']';return out.str();
}
std::string pair_fragment(RDDepict::EmbeddedFrag &first,RDDepict::EmbeddedFrag &second) {
  std::ostringstream out;out<<"[";fragment(out,first);out<<",";fragment(out,second);out<<"]";return out.str();
}
struct Capture {
  std::vector<int> ranks;bool enabled;std::string seeds="null",unembedded="null";std::vector<std::string> steps,pairs;std::string pending;
  void seeded(Fragments &values,const RDKit::INT_LIST &remaining) {
    seeds=fragments(values);unembedded=ids(remaining);
    if(!enabled)return;
    for(auto i=values.begin();i!=values.end();++i)for(auto j=values.begin();j!=values.end();++j) {
      if(i==j || pairs.size()>=64)continue;
      auto common=i->findCommonAtoms(*j);int target=-1,neighbor=-1;
      if(common.empty()) {
        for(const auto &[id,a]:i->GetEmbeddedAtoms())for(auto nbr:a.neighs)if(target<0 && j->GetEmbeddedAtoms().count(nbr)){target=id;neighbor=nbr;}
        if(target<0)continue;
      }
      auto first=*i,second=*j;std::ostringstream out;
      out<<"{\"before\":"<<pair_fragment(first,second)<<",\"common\":";integers(out,common);
      out<<",\"target\":"<<target<<",\"neighbor\":"<<neighbor<<",\"after\":";
      try { if(common.empty())first.mergeNoCommon(second,target,neighbor);else first.mergeWithCommon(second,common);out<<pair_fragment(first,second); }
      catch(const std::exception &){out<<"null";}
      out<<",\"common_after\":";integers(out,common);out<<'}';pairs.push_back(out.str());
    }
  }
  void before(Fragments &values,const RDKit::INT_LIST &remaining,Fragments::iterator master) {
    if(!enabled)return;
    pending="{\"before\":"+fragments(values)+",\"remaining\":"+ids(remaining)+",\"master\":"+std::to_string(std::distance(values.begin(),master));
  }
  void after(Fragments &values,const RDKit::INT_LIST &remaining) {
    if(!enabled)return;
    steps.push_back(pending+",\"after\":"+fragments(values)+",\"remaining_after\":"+ids(remaining)+"}");
  }
};
#ifdef RESHIKI_EXPANSION_WINDOWS_SOURCE
#include "depict-expansion-windows-source.inc"
#endif
#include "depict-expansion-observation.inc"
#ifdef RESHIKI_EXPANSION_WINDOWS_SOURCE
std::string full_outcome(const std::string &pickle,const RDGeom::INT_POINT2D_MAP *map,bool templates,bool canonical,bool source) {
  try {
    RDKit::RWMol mol;RDKit::MolPickler::molFromPickle(pickle,mol);
    RDDepict::Compute2DCoordParameters params;params.coordMap=map;params.useRingTemplates=templates;params.forceRDKit=true;params.canonOrient=canonical;
    const auto cid=source?RDDepict::expansionSourceFull(mol,params):RDDepict::compute2DCoords(mol,params);
    const auto &conformer=mol.getConformer(cid);std::ostringstream out;
    out<<"{\"kind\":\"success\",\"id\":"<<cid<<",\"is3d\":"<<(conformer.is3D()?"true":"false")<<",\"coordinates\":[";
    bool comma=false;for(const auto &p:conformer.getPositions()){if(comma)out<<',';comma=true;out<<'[';number(out,p.x);out<<',';number(out,p.y);out<<',';number(out,p.z);out<<']';}out<<"]}";return out.str();
  }catch(const std::exception &){return "{\"kind\":\"error\"}";}
}
#endif
void strings(std::ostream &out,const std::vector<std::string>&items) {out<<'[';bool comma=false;for(auto &s:items){if(comma)out<<',';comma=true;out<<s;}out<<']';}
void hex(std::ostream &out,const std::string &value) {out<<'"'<<std::hex<<std::setfill('0');for(unsigned char c:value)out<<std::setw(2)<<unsigned(c);out<<std::dec<<'"';}
int main() {
 try {
  RDKit::RWMol warm;warm.addAtom(new RDKit::Atom(6),true,true);Fragments ignored;std::vector<int> ranks{600};
  RDDepict::BOND_LEN=1.5;
  RDDepict::DepictorLocal::embedSquarePlanar(warm,warm.getAtomWithIdx(0),ignored,ranks);
  RDDepict::DepictorLocal::embedTBP(warm,warm.getAtomWithIdx(0),ignored,ranks);
  RDDepict::DepictorLocal::embedOctahedral(warm,warm.getAtomWithIdx(0),ignored,ranks);
  std::string line;
  while(std::getline(std::cin,line)) {
   std::istringstream input(line);std::string pickle,length;int count;unsigned templates,trace;
   if(!(input>>pickle>>length>>templates>>trace>>count)||count>100000||count< -1||pickle.size()%2)throw std::runtime_error("Invalid request");
   std::string binary;for(std::size_t i=0;i<pickle.size();i+=2)binary.push_back(char(16*digit(pickle.at(i))+digit(pickle.at(i+1))));
   RDKit::RWMol molecule;RDKit::MolPickler::molFromPickle(binary,molecule);
   RDGeom::INT_POINT2D_MAP coordinates;
   for(int i=0;i<count;++i){int id;std::string x,y;if(!(input>>id>>x>>y))throw std::runtime_error("Missing coordinate");coordinates[id]={std::bit_cast<double>(std::stoull(x,nullptr,16)),std::bit_cast<double>(std::stoull(y,nullptr,16))};}
   std::string trailing;if(input>>trailing)throw std::runtime_error("Trailing request");
   const auto *map=count<0?nullptr:&coordinates;
   RDDepict::BOND_LEN=std::bit_cast<double>(std::stoull(length,nullptr,16));
   std::ostringstream out;
#ifdef RESHIKI_EXPANSION_WINDOWS_SOURCE
   std::vector<std::string> public_outcomes;
   const auto requested_length=std::bit_cast<std::uint64_t>(RDDepict::BOND_LEN);
   for(unsigned canonical=0;canonical<2;++canonical){
     const auto original=full_outcome(binary,map,templates,bool(canonical),false);
     const auto adapted=full_outcome(binary,map,templates,bool(canonical),true);
     if(original!=adapted){std::cerr<<"Full native/source-adapter mismatch at request "<<public_outcomes.size()<<" input "<<line<<"\nNative "<<original<<"\nAdapter "<<adapted<<'\n';return 3;}
     if(std::bit_cast<std::uint64_t>(RDDepict::BOND_LEN)!=requested_length){std::cerr<<"Native/source wrapper changed shared BOND_LEN\n";return 3;}
     public_outcomes.push_back(original);
   }
#endif
   try {
    RDKit::RWMol direct(molecule);Fragments final;
#ifdef RESHIKI_EXPANSION_WINDOWS_SOURCE
    RDDepict::expansionSourceInitial(direct,map,final,templates);
#else
    RDDepict::computeInitialCoords(direct,map,final,templates);
#endif
    Fragments observed;Capture capture{{},bool(trace)};try {RDDepict::observedInitial(molecule,map,observed,templates,capture);} catch(...) {std::cerr<<"Observer failed after native accepted\n";return 2;}
    const auto expected=fragments(final);if(expected!=fragments(observed)){std::cerr<<"Observer altered native result\n";return 2;}
    std::string prepared;RDKit::MolPickler::pickleMol(direct,prepared,RDKit::PicklerOps::AllProps);
    out<<"{\"prepared\":";hex(out,prepared);out<<",\"fragments\":"<<expected<<",\"seeded\":"<<capture.seeds<<",\"unembedded\":"<<capture.unembedded<<",\"steps\":";strings(out,capture.steps);out<<",\"merges\":";strings(out,capture.pairs);
    bool invalid=false;for(auto &f:final)for(auto &[id,a]:f.GetEmbeddedAtoms())if(id>=direct.getNumAtoms()||!std::isfinite(a.loc.x)||!std::isfinite(a.loc.y))invalid=true;
    out<<",\"depict_ranks\":";integers(out,capture.ranks);
    out<<",\"final_boundary\":[";
    if(invalid)for(unsigned canonical=0;canonical<2;++canonical){if(canonical)out<<',';try {
      RDKit::RWMol output;RDKit::MolPickler::molFromPickle(binary,output);
      RDDepict::Compute2DCoordParameters p;p.coordMap=map;p.useRingTemplates=templates;p.forceRDKit=true;p.canonOrient=bool(canonical);
      auto cid=RDDepict::compute2DCoords(output,p);bool finite=true;
      for(const auto &point:output.getConformer(cid).getPositions())if(!std::isfinite(point.x)||!std::isfinite(point.y)||!std::isfinite(point.z))finite=false;
      out<<(finite?"\"finite\"":"\"nonfinite\"");
    }catch(const std::exception &){out<<"\"error\"";}}
    out<<"]}";
   } catch(const std::exception &error){std::cerr<<error.what()<<'\n';out.str("");out<<"null";}
#ifdef RESHIKI_EXPANSION_WINDOWS_SOURCE
   std::cout<<"{\"adapter_initial\":"<<out.str()<<",\"public_native\":";strings(std::cout,public_outcomes);std::cout<<"}\n";
#else
   std::cout<<out.str()<<'\n';
#endif
  }
 }catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
