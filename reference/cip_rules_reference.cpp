// Development-only calls to the pinned native comparison/sort APIs.
#include "cip_graph_observation.h"
#include <GraphMol/CIPLabeler/rules/Rules.h>
#include <GraphMol/CIPLabeler/rules/Rule1a.h>
#include <GraphMol/CIPLabeler/rules/Rule1b.h>
#include <GraphMol/CIPLabeler/rules/Rule2.h>
#include <GraphMol/CIPLabeler/rules/Rule3.h>
#include <GraphMol/CIPLabeler/rules/Rule4a.h>
#include <GraphMol/CIPLabeler/rules/Rule4c.h>
#include <GraphMol/CIPLabeler/rules/Rule5.h>
#include <GraphMol/CIPLabeler/rules/Rule6.h>
#include <GraphMol/CIPLabeler/rules/Rule4b.h>
#include <GraphMol/CIPLabeler/rules/Rule5New.h>
#include <algorithm>
#include <memory>

std::unique_ptr<SequenceRule> rule(unsigned id) {
  switch(id) {
    case 0: return std::make_unique<Rule1a>();
    case 1: return std::make_unique<Rule1b>();
    case 2: return std::make_unique<Rule2>();
    case 3: return std::make_unique<Rule3>();
    case 4: return std::make_unique<Rule4a>();
    case 5: return std::make_unique<Rule4c>();
    case 6: return std::make_unique<Rule5>();
    case 7: return std::make_unique<Rule6>();
    case 10: return std::make_unique<Rule4b>();
    case 11: return std::make_unique<Rule5New>();
    case 12: return std::make_unique<Rules>(std::initializer_list<SequenceRule*>{
      new Rule1a(),new Rule1b(),new Rule2(),new Rule3(),new Rule4a(),new Rule4b(),new Rule4c(),new Rule5New(),new Rule6()});
    case 13: return std::make_unique<Rule4b>(Descriptor::R);
    case 14: return std::make_unique<Rule4b>(Descriptor::S);
    case 15: return std::make_unique<Rule5New>(Descriptor::R);
    case 16: return std::make_unique<Rule5New>(Descriptor::S);
    case 17: return std::make_unique<Rule4b>(Descriptor::UNKNOWN);
    case 18: return std::make_unique<Rule5New>(Descriptor::UNKNOWN);
    default: {
      auto combined=std::make_unique<Rules>(std::initializer_list<SequenceRule*>{});
      for(unsigned i=0;i<(id==8?3:8);++i) combined->add(rule(i).release());
      return combined;
    }
  }
}
int main() {
  std::string line;
  while(std::getline(std::cin,line)) {
    try {
      unsigned root,atrop,seed,scheme,aux,reroot,deep,budget,operation; std::string hex;
      std::istringstream request(line);
      if(!(request>>root>>atrop>>seed>>scheme>>aux>>reroot>>deep>>budget>>operation>>hex) || hex.size()%2)
        throw std::runtime_error("Invalid request");
      std::string binary;
      for(std::size_t i=0;i<hex.size();i+=2) binary.push_back(char(16*digit(hex[i])+digit(hex[i+1])));
      RDKit::RWMol source;
      RDKit::MolPickler::molFromPickle(binary,source);
      CIPMol mol(source); Digraph graph(mol,mol.getAtom(root),atrop!=0);
      Registry registry; registry.add(graph.getOriginalRoot());
      graph.getOriginalRoot()->getEdges();
      if(aux || reroot) {
        graph.getNodes(mol.getAtom(mol.getNumAtoms()-1));
        View view(registry,graph.getOriginalRoot());
        if(aux) {
          unsigned count=aux==1?15:18;
          for(std::size_t i=0;i<view.nodes.size();++i) view.nodes[i]->setAux(Descriptor((i+seed)%count));
          for(std::size_t i=0;i<view.edges.size();++i) view.edges[i]->setAux(Descriptor((i*3+seed+1)%count));
          graph.setRule6Ref(mol.getAtom((root+1)%mol.getNumAtoms()));
        }
        if(reroot) graph.changeRoot(view.nodes.at(seed%view.nodes.size()));
      }
      auto node=graph.getCurrentRoot();
      auto edges=node->getEdges();
      if(seed%2) std::reverse(edges.begin(),edges.end());
      if(!edges.empty()) std::rotate(edges.begin(),edges.begin()+seed%edges.size(),edges.end());
      auto comparator=rule(scheme);
      // Public native entry point resets the thread-local recursive budget.
      RDKit::RWMol empty;
      assignCIPLabels(empty,budget);
      std::ostringstream result;
      if(operation==0) {
        if(edges.empty()) result<<"null";
        else result<<comparator->getComparision(edges.front(),edges.back(),deep!=0);
      } else if(operation==1) {
        auto priority=comparator->sort(node,edges,deep!=0);
        View view(registry,graph.getOriginalRoot());
        result<<"{\"edges\":"; view.edgeList(result,edges);
        result<<",\"unique\":"<<(priority.isUnique()?"true":"false")
              <<",\"pseudo\":"<<(priority.isPseudoAsymetric()?"true":"false")<<'}';
      } else {
        auto groups=comparator->getSorter()->getGroups(edges);
        View view(registry,graph.getOriginalRoot());
        result<<'[';
        for(std::size_t i=0;i<groups.size();++i) { if(i) result<<','; view.edgeList(result,groups[i]); }
        result<<']';
      }
      View view(registry,graph.getOriginalRoot());
      std::ostringstream out;
      out<<"{\"result\":"<<result.str()<<",\"graph\":";
      view.write(out,registry,graph,mol); out<<'}';
      std::cout<<out.str()<<std::endl;
    } catch(const MaxIterationsExceeded&) { std::cout<<"{\"error\":\"iterations\"}"<<std::endl;
    } catch(const TooManyNodesException&) { std::cout<<"{\"error\":\"nodes\"}"<<std::endl;
    } catch(const std::exception&) { std::cout<<"{\"error\":\"invalid\"}"<<std::endl; }
  }
}
