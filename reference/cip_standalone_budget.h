#pragma once
// Direct rule/configuration calls do not enter assignCIPLabels' scoped budget.
// These oracles compile the CIP sources without CIPLabeler.cpp and supply its
// comparison-budget hook, allowing exactly N calls (zero means unlimited).
#include <GraphMol/CIPLabeler/CIPLabeler.h>

namespace CIPOracle {
thread_local unsigned remainingComparisons = 0;
thread_local bool limitedComparisons = false;

void setComparisonBudget(unsigned budget) {
  remainingComparisons = budget;
  limitedComparisons = budget != 0;
}
}  // namespace CIPOracle

namespace RDKit::CIPLabeler_detail {
bool decrementRemainingCallCountAndCheck() {
  if (!CIPOracle::limitedComparisons) return true;
  if (CIPOracle::remainingComparisons == 0) return false;
  --CIPOracle::remainingComparisons;
  return true;
}
}  // namespace RDKit::CIPLabeler_detail
