#pragma once
// Development-only observations of the pinned native RDKit Digraph API.
// Registry traversal only assigns stable IDs for serialization; all chemical
// expansion, masses, flags, re-rooting and edge filtering come from RDKit.
#include <GraphMol/MolPickler.h>
#include <GraphMol/CIPLabeler/CIPMol.h>
#include <GraphMol/CIPLabeler/Digraph.h>
#include <GraphMol/CIPLabeler/Node.h>
#include <GraphMol/CIPLabeler/Edge.h>
#include <bit>
#include <cstdint>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

using namespace RDKit::CIPLabeler;
struct Registry {
  std::vector<Node*> nodes;
  std::vector<Edge*> edges;
  std::unordered_set<Node*> seenNodes;
  std::unordered_set<Edge*> seenEdges;
  std::unordered_map<Node*, std::vector<Edge*>> adjacent;
  void add(Node* n) { if (seenNodes.insert(n).second) nodes.push_back(n); }
  void refresh() {
    for (std::size_t i=0; i<nodes.size(); ++i) {
      auto n=nodes[i];
      if (!n->isExpanded()) continue;
      for (auto e : n->getEdges()) {
        if (!seenEdges.insert(e).second) continue;
        edges.push_back(e);
        add(e->getBeg()); add(e->getEnd());
        adjacent[e->getBeg()].push_back(e);
        adjacent[e->getEnd()].push_back(e);
      }
    }
  }
  const std::vector<Edge*>& incident(Node* n) {
    return n->isExpanded() ? n->getEdges() : adjacent[n];
  }
};
struct View {
  std::vector<Node*> nodes;
  std::vector<Edge*> edges;
  std::unordered_map<Node*,std::size_t> ids;
  std::unordered_map<Edge*,std::size_t> eids;
  View(Registry& registry, Node* origin) {
    registry.refresh();
    nodes.push_back(origin); ids.emplace(origin,0);
    for (std::size_t i=0; i<nodes.size(); ++i) {
      auto n=nodes[i];
      for (auto e : registry.incident(n)) {
        if (eids.emplace(e,edges.size()).second) edges.push_back(e);
        auto other=e->getOther(n);
        if (ids.emplace(other,nodes.size()).second) nodes.push_back(other);
      }
    }
  }
  void edgeList(std::ostream& out, const std::vector<Edge*>& values) const {
    out << '[';
    for (std::size_t i=0;i<values.size();++i) { if (i) out << ','; out << eids.at(values[i]); }
    out << ']';
  }
  void write(std::ostream& out, Registry& registry, Digraph& graph, CIPMol& mol) const {
    out << "{\"root\":" << ids.at(graph.getCurrentRoot()) << ",\"nodes\":[";
    for (std::size_t i=0;i<nodes.size();++i) {
      if (i) out << ',';
      auto n=nodes[i]; auto f=n->getAtomicNumFraction();
      unsigned flags=0;
      for (int bit : {1,2,4,8}) if (n->isSet(bit)) flags |= bit;
      out << '[' << (n->getAtom() ? int(n->getAtom()->getIdx()) : -1) << ','
          // Native visit distances are C char bytes; signedness is platform ABI.
          << (n->isDuplicate() ? int(static_cast<unsigned char>(n->getDistance())) : n->getDistance()) << ','
          << f.numerator() << ',' << f.denominator() << ',' << n->getAtomicNum() << ','
          << n->getMassNum() << ',' << std::bit_cast<std::uint64_t>(n->getAtomicMass()) << ','
          << flags << ',' << (n->isTerminal() ? "true" : "false") << ',' << int(n->getAux()) << ',';
      edgeList(out,registry.incident(n)); out << ']';
    }
    out << "],\"edges\":[";
    for (std::size_t i=0;i<edges.size();++i) {
      if (i) out << ',';
      auto e=edges[i];
      out << '[' << ids.at(e->getBeg()) << ',' << ids.at(e->getEnd()) << ','
          << (e->getBond() ? int(e->getBond()->getIdx()) : -1) << ',' << int(e->getAux()) << ']';
    }
    out << "],\"seen\":[";
    for (unsigned i=0;i<mol.getNumAtoms();++i) {
      if (i) out << ','; out << (graph.seenAtom(mol.getAtom(i)) ? "true" : "false");
    }
    out << "],\"rule6\":" << (graph.getRule6Ref() ? int(graph.getRule6Ref()->getIdx()) : -1) << '}';
  }
};
int digit(char c) {
  if (c >= '0' && c <= '9') return c-'0';
  if (c >= 'a' && c <= 'f') return c-'a'+10;
  throw std::runtime_error("Invalid hexadecimal pickle");
}
