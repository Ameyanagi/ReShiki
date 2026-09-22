// Development-only direct native CIP Digraph observations.
#include "cip_graph_observation.h"
int main() {
  std::string line;
  while (std::getline(std::cin,line)) {
    try {
      std::istringstream request(line);
      unsigned root, atrop, seed; std::string hex;
      if (!(request >> root >> atrop >> seed >> hex) || hex.size()%2)
        throw std::runtime_error("Invalid request");
      std::string binary;
      for (std::size_t i=0;i<hex.size();i+=2) binary.push_back(char(16*digit(hex[i])+digit(hex[i+1])));
      RDKit::RWMol molecule;
      RDKit::MolPickler::molFromPickle(binary,molecule);
      CIPMol mol(molecule);
      Digraph graph(mol,mol.getAtom(root),atrop != 0);
      Registry registry; registry.add(graph.getOriginalRoot());
      std::ostringstream out;
      out << "{\"states\":[";
      View(registry,graph.getOriginalRoot()).write(out,registry,graph,mol);
      graph.getOriginalRoot()->getEdges();
      out << ','; View(registry,graph.getOriginalRoot()).write(out,registry,graph,mol);
      if (!graph.getOriginalRoot()->getEdges().empty())
        graph.changeRoot(graph.getOriginalRoot()->getEdges().front()->getEnd());
      out << ','; View(registry,graph.getOriginalRoot()).write(out,registry,graph,mol);
      auto target=mol.getAtom(mol.getNumAtoms()-1);
      graph.getNodes(target);
      View full(registry,graph.getOriginalRoot());
      out << ','; full.write(out,registry,graph,mol);
      graph.changeRoot(full.nodes.at(seed % full.nodes.size()));
      graph.setRule6Ref(mol.getAtom((root+1)%mol.getNumAtoms()));
      for (std::size_t i=0;i<full.nodes.size();++i) full.nodes[i]->setAux(Descriptor(i%18));
      for (std::size_t i=0;i<full.edges.size();++i) full.edges[i]->setAux(Descriptor((i+3)%18));
      out << ','; full.write(out,registry,graph,mol);
      out << "],\"find\":[";
      auto found=graph.getNodes(target);
      for (std::size_t i=0;i<found.size();++i) { if(i) out << ','; out << full.ids.at(found[i]); }
      out << "],\"out\":[";
      for (std::size_t i=0;i<full.nodes.size();++i) {
        if(i) out << ','; full.edgeList(out,full.nodes[i]->getNonTerminalOutEdges());
      }
      out << "],\"to\":[";
      for (std::size_t i=0;i<full.nodes.size();++i) {
        if(i) out << ','; full.edgeList(out,full.nodes[i]->getEdges(target));
      }
      out << "]}";
      std::cout << out.str() << std::endl;
    } catch (const TooManyNodesException&) {
      std::cout << "{\"error\":\"nodes\"}" << std::endl;
    } catch (const std::exception&) {
      std::cout << "{\"error\":\"invalid\"}" << std::endl;
    }
  }
}
