// Direct pinned-native coordination, coordinate-map and stereobond seed APIs.
// The builder changes the single ` private:` line in EmbeddedFrag.h to
// ` public:` for state observation only; no declaration or ABI is changed.
#include <GraphMol/MolPickler.h>
#include <GraphMol/RWMol.h>
#include <GraphMol/Chirality.h>
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
namespace RDDepict::DepictorLocal {
void embedSquarePlanar(const RDKit::ROMol &, const RDKit::Atom *, std::list<EmbeddedFrag> &,const std::vector<int> &);
void embedTBP(const RDKit::ROMol &, const RDKit::Atom *, std::list<EmbeddedFrag> &,const std::vector<int> &);
void embedOctahedral(const RDKit::ROMol &, const RDKit::Atom *, std::list<EmbeddedFrag> &,const std::vector<int> &);
std::vector<const RDKit::Atom *> getRankedAtomNeighbors(const RDKit::ROMol &, const RDKit::Atom *,const std::vector<int> &);
}
double read_number(std::istream &input){std::string value;if(!(input>>value))throw std::runtime_error("Missing number");return std::bit_cast<double>(std::stoull(value,nullptr,16));}
int main(){
  try {
    std::string line;std::vector<double> lengths;
    while(std::getline(std::cin,line)){
      std::istringstream input(line);std::vector<double> captured;
      for(unsigned i=0;i<3;++i)captured.push_back(read_number(input));
      if(lengths.empty()){
        // Each native function captures BOND_LEN before checking its atom tag.
        // Warm each static with a nonmatching tag, then use explicit current length.
        RDKit::RWMol dummy;dummy.addAtom(new RDKit::Atom(6),true,true);
        std::list<RDDepict::EmbeddedFrag> ignored;std::vector<int> ranks{0};
        RDDepict::BOND_LEN=captured.at(0);RDDepict::DepictorLocal::embedSquarePlanar(dummy,dummy.getAtomWithIdx(0),ignored,ranks);
        RDDepict::BOND_LEN=captured.at(1);RDDepict::DepictorLocal::embedTBP(dummy,dummy.getAtomWithIdx(0),ignored,ranks);
        RDDepict::BOND_LEN=captured.at(2);RDDepict::DepictorLocal::embedOctahedral(dummy,dummy.getAtomWithIdx(0),ignored,ranks);
        lengths=captured;
      }else if(captured!=lengths)throw std::runtime_error("Native static geometry cannot be reset");
      RDDepict::BOND_LEN=read_number(input);
      std::string pickle,mode;unsigned seed,count;if(!(input>>pickle>>mode>>seed>>count)||count>100000||pickle.size()%2)throw std::runtime_error("Invalid input");
      std::string binary;for(std::size_t i=0;i<pickle.size();i+=2)binary.push_back(char(16*digit(pickle.at(i))+digit(pickle.at(i+1))));
      RDKit::RWMol molecule;RDKit::MolPickler::molFromPickle(binary,molecule);
      std::vector<int> ranks(count);for(auto &rank:ranks)if(!(input>>rank))throw std::runtime_error("Missing rank");
      if(count!=molecule.getNumAtoms())throw std::runtime_error("Wrong rank count");
      if(!(input>>count)||count>100000)throw std::runtime_error("Invalid coordinates");
      RDGeom::INT_POINT2D_MAP coordinates;
      for(unsigned i=0;i<count;++i){int id;if(!(input>>id))throw std::runtime_error("Missing index");double x=read_number(input),y=read_number(input);coordinates[id]={x,y};}
      std::ostringstream result;result<<"{\"fragment\":";
      std::list<RDDepict::EmbeddedFrag> fragments;bool failure=false;
      try {
        if(mode=="coordinates")fragments.emplace_back(&molecule,coordinates);
        else if(mode=="bond")fragments.emplace_back(molecule.getBondWithIdx(seed));
        else if(mode=="sp")RDDepict::DepictorLocal::embedSquarePlanar(molecule,molecule.getAtomWithIdx(seed),fragments,ranks);
        else if(mode=="tbp")RDDepict::DepictorLocal::embedTBP(molecule,molecule.getAtomWithIdx(seed),fragments,ranks);
        else if(mode=="oh")RDDepict::DepictorLocal::embedOctahedral(molecule,molecule.getAtomWithIdx(seed),fragments,ranks);
        else throw std::runtime_error("Invalid mode");
      }catch(const std::exception &){failure=true;}
      if(fragments.empty())result<<"null";else fragment(result,fragments.front());
      result<<",\"error\":"<<(failure?"true":"false");
      if(mode=="sp"||mode=="tbp"||mode=="oh"){
        const auto center=molecule.getAtomWithIdx(seed);
        RDKit::INT_VECT ranked;for(auto atom:RDDepict::DepictorLocal::getRankedAtomNeighbors(molecule,center,ranks))ranked.push_back(atom->getIdx());
        result<<",\"ranked\":";integers(result,ranked);
        RDKit::INT_VECT across,axial;
        for(auto atom:molecule.atomNeighbors(center)){auto opposite=RDKit::Chirality::getChiralAcrossAtom(center,atom);across.push_back(opposite?int(opposite->getIdx()):-1);}
        for(int direction:{1,-1}){auto atom=RDKit::Chirality::getTrigonalBipyramidalAxialAtom(center,direction);axial.push_back(atom?int(atom->getIdx()):-1);}
        result<<",\"across\":";integers(result,across);result<<",\"axial\":";integers(result,axial);
        result<<",\"angles\":[";bool comma=false;
        for(auto first:molecule.atomNeighbors(center))for(auto second:molecule.atomNeighbors(center)){if(comma)result<<',';comma=true;number(result,RDKit::Chirality::getIdealAngleBetweenLigands(center,first,second));}
        result<<']';
      }
      result<<'}';std::cout<<result.str()<<'\n';
    }
  }catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
