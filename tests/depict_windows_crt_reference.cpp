// Independent runtime witness. Test tooling only; never linked into ReShiki.
#include <windows.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>

namespace {
using Unary = double(__cdecl*)(double);
std::uint64_t bits(double value) {
  std::uint64_t result = 0;
  std::memcpy(&result, &value, sizeof(result));
  return result;
}
double value(std::uint64_t encoded) {
  double result = 0;
  std::memcpy(&result, &encoded, sizeof(result));
  return result;
}
}
int main() {
  const auto runtime = GetModuleHandleW(L"ucrtbase.dll");
  if (!runtime) return 1;
  const auto acos_fn = reinterpret_cast<Unary>(GetProcAddress(runtime, "acos"));
  const auto sin_fn = reinterpret_cast<Unary>(GetProcAddress(runtime, "sin"));
  const auto cos_fn = reinterpret_cast<Unary>(GetProcAddress(runtime, "cos"));
  if (!acos_fn || !sin_fn || !cos_fn) return 2;
  std::array<wchar_t, 32768> wide{};
  const auto length = GetModuleFileNameW(runtime, wide.data(), static_cast<DWORD>(wide.size()));
  if (!length || length >= wide.size()) return 3;
  std::array<char, 131072> utf8{};
  if (!WideCharToMultiByte(CP_UTF8, 0, wide.data(), -1, utf8.data(),
                           static_cast<int>(utf8.size()), nullptr, nullptr)) return 4;
  const auto angle = acos_fn(value(0x3fd954d5989f7ab5));
  const auto phase = value(0x4000c152382d7365);
  std::printf("module: %s\n", utf8.data());
  std::printf("primitive_bits: %016llx %016llx %016llx\n",
              bits(angle), bits(sin_fn(angle)), bits(cos_fn(angle)));
  std::printf("ring_bits: %016llx %016llx %016llx %016llx\n",
              bits(sin_fn(phase)), bits(cos_fn(phase)),
              bits(sin_fn(2.0 * phase)), bits(cos_fn(2.0 * phase)));
  return 0;
}
