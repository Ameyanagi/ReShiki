// Direct pinned-native template matcher and template-assisted constructor.
// Only private header access is exposed by the builder; executable methods are
// from the original RDKit wheel. The observed trace is separately extracted
// from the pinned implementation, without changing its matching conditions.
#include "depict-native-fragment-observation.inc"
#include <GraphMol/Depictor/Templates.h>
#include <GraphMol/Substruct/SubstructMatch.h>
#include <GraphMol/QueryAtom.h>
#include "depict-template-observation.inc"

void trace(std::ostream &out,const RDKit::ROMol &mol,const RDKit::INT_VECT &atoms) {
  auto [slot,match]=RDDepict::observedMatch(&mol,atoms);
  out<<"{\"slot\":"<<slot<<",\"mapping\":[";
  bool comma=false;
  for(auto [query,target]:match){if(comma)out<<',';comma=true;out<<'['<<query<<','<<target<<']';}
  out<<"],\"fragment\":";
  RDDepict::EmbeddedFrag value;value.dp_mol=&mol;
  if(value.matchToTemplate(atoms))fragment(out,value);else out<<"null";
  out<<'}';
}
void alternatives(std::ostream &out,const RDKit::ROMol &mol,const RDKit::INT_VECT &atoms) {
  RDKit::RWMol masked(mol,true);std::vector<bool> included(mol.getNumAtoms(),false);
  for(auto id:atoms)included.at(id)=true;
  for(auto atom:masked.atoms())if(!included.at(atom->getIdx()))atom->setAtomicNum(200);
  unsigned bonds=0;for(auto bond:masked.bonds())if(included.at(bond->getBeginAtomIdx())&&included.at(bond->getEndAtomIdx()))++bonds;
  auto &catalog=RDDepict::CoordinateTemplates::getRingSystemTemplates();
  out<<'[';bool comma=false;int slot=-1;
  for(const auto &query:catalog.getMatchingTemplates(atoms.size())) {
    ++slot;if(query->getNumBonds()!=bonds)continue;
    RDKit::SubstructMatchParameters parameters;parameters.uniquify=false;parameters.maxMatches=256;
    auto matches=RDKit::SubstructMatch(masked,*query,parameters);
    if(matches.size()<2 || RDDepict::checkStereoChemistry(masked,*query,matches.front()))continue;
    bool later=false;for(std::size_t i=1;i<matches.size();++i)if(RDDepict::checkStereoChemistry(masked,*query,matches.at(i))){later=true;break;}
    if(later){if(comma)out<<',';comma=true;out<<slot;}
  }
  out<<']';
}
int main() {
  try {
    std::string line;
    while(std::getline(std::cin,line)) {
      std::istringstream input(line);
      std::string pickle,length;unsigned count,diagnostic;
      if(!(input>>pickle>>length>>count)||count>100000||pickle.size()%2)throw std::runtime_error("Invalid request");
      std::string binary;
      for(std::size_t i=0;i<pickle.size();i+=2)binary.push_back(char(16*digit(pickle.at(i))+digit(pickle.at(i+1))));
      RDKit::RWMol molecule;RDKit::MolPickler::molFromPickle(binary,molecule);
      RDKit::INT_VECT atoms(count);for(auto &id:atoms)if(!(input>>id))throw std::runtime_error("Missing atom");
      if(!(input>>count)||count>100000)throw std::runtime_error("Invalid rings");
      RDKit::VECT_INT_VECT rings(count);
      for(auto &ring:rings){unsigned n;if(!(input>>n)||n>100000)throw std::runtime_error("Invalid ring");ring.resize(n);for(auto &id:ring)if(!(input>>id))throw std::runtime_error("Missing ring atom");}
      if(!(input>>diagnostic))throw std::runtime_error("Missing diagnostic flag");
      std::string trailing;if(input>>trailing)throw std::runtime_error("Trailing input");
      RDDepict::BOND_LEN=std::bit_cast<double>(std::stoull(length,nullptr,16));
      std::ostringstream out;out<<"{\"match\":";
      try{std::ostringstream observed;trace(observed,molecule,atoms);out<<observed.str();}catch(const std::exception &){out<<"null";}
      RDKit::INT_VECT core_ids;
      auto core=RDDepict::findCoreRings(rings,core_ids,molecule);
      out<<",\"core\":";integers(out,core_ids);
      out<<",\"core_match\":";
      if(core.size()>1 && core.size()<rings.size()){
        RDKit::INT_VECT core_atoms;RDKit::Union(core,core_atoms);
        try{std::ostringstream observed;trace(observed,molecule,core_atoms);out<<observed.str();}catch(const std::exception &){out<<"null";}
      }else out<<"null";
      out<<",\"embedded\":";
      try{RDDepict::EmbeddedFrag value(&molecule,rings,true);fragment(out,value);}catch(const std::exception &){out<<"null";}
      out<<",\"later_compatible\":";
      if(diagnostic)alternatives(out,molecule,atoms);else out<<"[]";
      out<<'}';std::cout<<out.str()<<'\n';
    }
  }catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
