// Development-only oracle: compile with the platform's C++ standard library
// and save stdout as tests/fixtures/smiles-sort-{linux,macos,windows}.json.
// The tuple and key-only comparator match Canon.cpp's traversal candidates.
#include <algorithm>
#include <cstdint>
#include <iostream>
#include <tuple>
#include <vector>

using Entry = std::tuple<std::int32_t, std::size_t, std::size_t>;
struct Compare {
  bool operator()(const Entry &a, const Entry &b) const {
    return std::get<0>(a) < std::get<0>(b);
  }
};

std::vector<Entry> input(std::size_t n, unsigned pattern) {
  std::vector<Entry> values;
  std::uint32_t random = 39103;
  for (std::size_t i = 0; i < n; ++i) {
    random = random * 1664525u + 1013904223u;
    std::int32_t key = 0;
    switch (pattern) {
    case 0: key = i / 3; break;
    case 1: key = (n - i) / 3; break;
    case 2: key = 0; break;
    case 3: key = i % 3; break;
    case 4: key = std::min(i, n - i); break;
    case 5: key = random % 7; break;
    case 6: key = random % 31; break;
    case 7: key = std::int64_t(random) - 2147483648LL; break;
    case 8: key = (i % 17) / 2; break;
    case 9: key = i == n / 2 ? 0 : i; break;
    case 10: key = i % 2; break;
    case 11: key = (random % 2 == 0) ? i / 2 : (n - i) / 2; break;
    }
    values.emplace_back(key, i, i);
  }
  return values;
}

std::uint64_t fingerprint(const std::vector<Entry> &values) {
  std::uint64_t hash = 14695981039346656037ULL;
  for (const auto &value : values) {
    const auto id = std::uint64_t(std::get<1>(value));
    for (unsigned shift = 0; shift < 64; shift += 8) {
      hash = (hash ^ ((id >> shift) & 255)) * 1099511628211ULL;
    }
  }
  return hash;
}

int main() {
  std::cout << "{\"library\":\"";
#if defined(_LIBCPP_VERSION)
  std::cout << "libc++ " << _LIBCPP_VERSION;
#elif defined(_MSVC_STL_VERSION)
  std::cout << "MSVC STL " << _MSVC_STL_VERSION;
#elif defined(__GLIBCXX__)
  std::cout << "libstdc++ " << __GLIBCXX__;
#endif
  std::cout << "\",\"cases\":[\n";
  bool first = true;
  for (std::size_t n : {0, 1, 2, 3, 4, 5, 15, 16, 17, 23, 24, 25, 31, 32, 33,
                        40, 41, 42, 63, 64, 65, 127, 128, 129, 130, 255, 512, 2048}) {
    for (unsigned pattern = 0; pattern < 12; ++pattern) {
      auto sorted = input(n, pattern);
      auto heap = sorted;
      std::sort(sorted.begin(), sorted.end(), Compare{});
      std::make_heap(heap.begin(), heap.end(), Compare{});
      std::sort_heap(heap.begin(), heap.end(), Compare{});
      if (!first) std::cout << ",\n";
      first = false;
      std::cout << "[" << n << "," << pattern << ",\"" << fingerprint(sorted)
                << "\",\"" << fingerprint(heap) << "\"]";
    }
  }
  std::cout << "\n]}\n";
}
