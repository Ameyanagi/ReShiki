// Direct calls into the pinned native wheel, with visibility-only observation.
#include "depict-native-fragment-observation.inc"
#include <GraphMol/MolOps.h>
#include <cmath>
#include <numeric>

double read_number(std::istream &input) {
  std::string text;
  if(!(input>>text))throw std::runtime_error("Missing number");
  return std::bit_cast<double>(std::stoull(text,nullptr,16));
}
void pairs(std::ostream &out,const std::vector<RDDepict::PAIR_I_I> &values) {
  out<<'[';bool comma=false;
  for(auto [a,b]:values){if(comma)out<<',';comma=true;out<<'['<<a<<','<<b<<']';}
  out<<']';
}
// Windows does not export totalDensity. This is the exact source fold from
// EmbeddedFrag.cpp; collision and repair methods still run in the native DLL.
// On Linux/macOS compare the fold directly with the exported native function.
double observed_density(RDDepict::EmbeddedFrag &value) {
  auto result = std::accumulate(value.d_eatoms.begin(), value.d_eatoms.end(), 0.0,
    [](double accum, auto &dea) { return dea.second.d_density + accum; });
#ifndef _WIN32
  if(std::bit_cast<std::uint64_t>(result)!=std::bit_cast<std::uint64_t>(value.totalDensity()))
    throw std::runtime_error("Native density fold mismatch");
#endif
  return result;
}
int main() {
  try {
    std::string line;
    while(std::getline(std::cin,line)) {
      std::istringstream input(line);std::string pickle;unsigned count,done;
      if(!(input>>pickle>>count>>done)||count>100000||pickle.size()%2)throw std::runtime_error("Invalid request");
      std::string binary;
      for(std::size_t i=0;i<pickle.size();i+=2)binary.push_back(char(16*digit(pickle.at(i))+digit(pickle.at(i+1))));
      RDKit::RWMol molecule;RDKit::MolPickler::molFromPickle(binary,molecule);
      RDDepict::EmbeddedFrag initial;initial.dp_mol=&molecule;if(done)initial.markDone();
      initial.d_px=read_number(input);initial.d_nx=read_number(input);initial.d_py=read_number(input);initial.d_ny=read_number(input);
      for(unsigned i=0;i<count;++i) {
        unsigned key,ccw,fixed;RDDepict::EmbeddedAtom atom;
        if(!(input>>key>>atom.aid>>atom.nbr1>>atom.nbr2>>atom.CisTransNbr>>ccw>>atom.rotDir>>fixed))throw std::runtime_error("Invalid atom");
        atom.ccw=ccw;atom.df_fixed=fixed;
        atom.loc.x=read_number(input);atom.loc.y=read_number(input);atom.normal.x=read_number(input);atom.normal.y=read_number(input);atom.angle=read_number(input);atom.d_density=read_number(input);
        unsigned n;if(!(input>>n)||n>100000)throw std::runtime_error("Invalid neighbors");
        atom.neighs.resize(n);for(auto &id:atom.neighs)input>>id;
        initial.d_eatoms.emplace(key,atom);
      }
      unsigned n;if(!(input>>n)||n>100000)throw std::runtime_error("Invalid attachments");
      initial.d_attachPts.resize(n);for(auto &id:initial.d_attachPts)input>>id;
      std::string extra;if(input>>extra)throw std::runtime_error("Trailing request");
      std::ostringstream out;out<<"{\"initial\":";fragment(out,initial);
      for(bool include:{false,true}) {
        auto value=initial;auto collisions=value.findCollisions(RDKit::MolOps::getDistanceMat(molecule),include);
        out<<(include?",\"find_bonds\":{":",\"find_atoms\":{")<<"\"fragment\":";fragment(out,value);
        out<<",\"density\":";number(out,observed_density(value));out<<",\"pairs\":";pairs(out,collisions);out<<'}';
      }
      for(const auto &mode:{"flip","open","shorten","combined"}) {
        auto value=initial;out<<",\""<<mode<<"\":{";
        bool failed=false;
        try {
          const std::string op(mode);
          if(op=="flip"||op=="combined")value.removeCollisionsBondAndSpiroFlip();
          if(op=="open"||op=="combined")value.removeCollisionsOpenAngles();
          if(op=="shorten"||op=="combined")value.removeCollisionsShortenBonds();
        }catch(const std::exception &){failed=true;}
        out<<"\"error\":"<<(failed?"true":"false")<<",\"fragment\":";fragment(out,value);out<<'}';
      }
      out<<'}';std::cout<<out.str()<<'\n';
    }
  }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}
}
