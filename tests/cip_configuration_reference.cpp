// Development-only direct calls to pinned native Configuration APIs.
#include "cip_graph_observation.h"
#include <GraphMol/CIPLabeler/configs/Tetrahedral.h>
#include <GraphMol/CIPLabeler/configs/Sp2Bond.h>
#include <GraphMol/CIPLabeler/configs/AtropisomerBond.h>
#include <GraphMol/CIPLabeler/rules/Rules.h>
#include <GraphMol/CIPLabeler/rules/Rule1a.h>
#include <GraphMol/CIPLabeler/rules/Rule1b.h>
#include <GraphMol/CIPLabeler/rules/Rule2.h>
#include <GraphMol/CIPLabeler/rules/Rule3.h>
#include <GraphMol/CIPLabeler/rules/Rule4a.h>
#include <GraphMol/CIPLabeler/rules/Rule4b.h>
#include <GraphMol/CIPLabeler/rules/Rule4c.h>
#include <GraphMol/CIPLabeler/rules/Rule5New.h>
#include <GraphMol/CIPLabeler/rules/Rule6.h>
#include <memory>

int main() {
  std::string line;
  while (std::getline(std::cin,line)) {
    try {
      std::istringstream request(line);
      int kind; request >> kind;
      if (kind==3) {
        unsigned a,b; request>>a>>b;
        std::vector<unsigned> first,second;
        for(unsigned i=0;i<4;++i) { first.push_back(a%4); a/=4; second.push_back(b%4); b/=4; }
        std::cout<<Configuration::parity4(first,second)<<std::endl;
        continue;
      }
      unsigned target,cfg,seed,full,aux,budget,repeat,reverse; int origin,nodeAtom,write;
      std::string hex;
      if(!(request>>target>>cfg>>origin>>nodeAtom>>seed>>full>>aux>>budget>>repeat>>reverse>>write>>hex))
        throw std::runtime_error("Invalid request");
      std::string binary;
      for(std::size_t i=0;i<hex.size();i+=2) binary.push_back(char(16*digit(hex[i])+digit(hex.at(i+1))));
      RDKit::RWMol source; RDKit::MolPickler::molFromPickle(binary,source); CIPMol mol(source);
      std::unique_ptr<Configuration> config;
      RDKit::Bond* bond=nullptr;
      RDKit::Atom* atom=nullptr;
      if(kind==0) { atom=mol.getAtom(target); config=std::make_unique<Tetrahedral>(mol,atom); }
      else {
        bond=mol.getBond(target); auto a=bond->getBeginAtom(); auto b=bond->getEndAtom();
        if(reverse) std::swap(a,b);
        if(kind==1) config=std::make_unique<Sp2Bond>(mol,bond,a,b,RDKit::Bond::BondStereo(cfg));
        else config=std::make_unique<AtropisomerBond>(mol,bond,a,b,RDKit::Bond::BondStereo(cfg));
      }
      std::unique_ptr<Digraph> external;
      if(origin>=0) external=std::make_unique<Digraph>(mol,mol.getAtom(origin),kind==2);
      auto &graph=external?*external:config->getDigraph();
      Registry registry; registry.add(graph.getOriginalRoot());
      Node* node=graph.getOriginalRoot();
      if(aux || nodeAtom>=0 || repeat) {
        graph.getNodes(mol.getAtom(mol.getNumAtoms()-1));
        View view(registry,graph.getOriginalRoot());
        if(aux) {
          for(std::size_t i=0;i<view.nodes.size();++i) {
            auto n=view.nodes[i];
            auto tag=n->getAtom()?n->getAtom()->getChiralTag():RDKit::Atom::CHI_UNSPECIFIED;
            auto desc=aux==3?(tag==RDKit::Atom::CHI_TETRAHEDRAL_CW?Descriptor::R:tag==RDKit::Atom::CHI_TETRAHEDRAL_CCW?Descriptor::S:Descriptor::NONE):aux==1 ? Descriptor((i+seed)%15) : Descriptor(3+(i+seed)%4);
            n->setAux(desc);
          }
          for(std::size_t i=0;i<view.edges.size();++i) view.edges[i]->setAux(aux==3?Descriptor::NONE:Descriptor((i*3+seed+1)%15));
          graph.setRule6Ref(mol.getAtom((target+1)%mol.getNumAtoms()));
        }
        if(nodeAtom>=0) {
          auto nodes=graph.getNodes(mol.getAtom(nodeAtom));
          std::vector<Node*> real;
          for(auto n:nodes) if(!n->isDuplicateOrH()) real.push_back(n);
          if(real.empty()) throw std::runtime_error("Missing auxiliary node");
          node=real.at(seed%real.size());
        }
        if(repeat) graph.changeRoot(view.nodes.at(seed%view.nodes.size()));
      }
      Rules rules({new Rule1a(),new Rule1b(),new Rule2()});
      if(full) { rules.add(new Rule3());rules.add(new Rule4a());rules.add(new Rule4b());rules.add(new Rule4c());rules.add(new Rule5New());rules.add(new Rule6()); }
      RDKit::RWMol empty; assignCIPLabels(empty,budget);
      std::ostringstream out; out<<"{\"foci\":[";
      for(std::size_t i=0;i<config->getFoci().size();++i) { if(i)out<<',';out<<config->getFoci()[i]->getIdx(); }
      out<<"],\"carriers\":[";
      for(std::size_t i=0;i<config->getCarriers().size();++i) { if(i)out<<',';auto a=config->getCarriers()[i];out<<(a?int(a->getIdx()):-1); }
      out<<"],\"results\":[";
      for(unsigned pass=0;pass<(repeat?2u:1u);++pass) {
        if(pass)out<<',';
        auto desc=origin>=0 || nodeAtom>=0 ? config->label(node,graph,rules) : config->label(rules);
        auto chosen=write>=0?Descriptor(write):(kind==0?Descriptor::R:kind==1?Descriptor::E:Descriptor::M);
        auto hasBefore=config->hasPrimaryLabel();
        config->setPrimaryLabel(chosen);
        std::vector<unsigned> ranks; std::string code;
        const RDKit::RDProps& props=kind==0?static_cast<const RDKit::RDProps&>(*atom):static_cast<const RDKit::RDProps&>(*bond);
        props.getProp(RDKit::common_properties::_CIPNeighborOrder,ranks);
        props.getProp(RDKit::common_properties::_CIPCode,code);
        auto hasSet=config->hasPrimaryLabel(); config->resetPrimaryLabel();
        out<<"{\"descriptor\":"<<int(desc)<<",\"code\":\""<<code<<"\",\"neighbor_order\":[";
        for(std::size_t i=0;i<ranks.size();++i) { if(i)out<<',';out<<(ranks[i]==IMPLICITH?-1:static_cast<long long>(ranks[i])); }
        out<<"],\"has\":["<<(hasBefore?"true":"false")<<','<<(hasSet?"true":"false")<<','<<(config->hasPrimaryLabel()?"true":"false")<<"],\"stereo\":";
        if(bond) {
          out<<'['<<int(bond->getStereo())<<",[";
          for(std::size_t i=0;i<bond->getStereoAtoms().size();++i) { if(i)out<<',';out<<bond->getStereoAtoms()[i]; }
          out<<"]]";
        } else out<<"null";
        out<<",\"graph\":"; View view(registry,graph.getOriginalRoot()); view.write(out,registry,graph,mol); out<<'}';
      }
      out<<"]}";std::cout<<out.str()<<std::endl;
    } catch(const MaxIterationsExceeded&) { std::cout<<"{\"error\":\"iterations\"}"<<std::endl;
    } catch(const TooManyNodesException&) { std::cout<<"{\"error\":\"nodes\"}"<<std::endl;
    } catch(const std::exception&) { std::cout<<"{\"error\":\"invalid\"}"<<std::endl; }
  }
}
