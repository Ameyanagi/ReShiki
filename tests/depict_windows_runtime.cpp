// Native-observer-only process configuration; never linked into ReShiki.
#include <cstdio>
#include <cstdlib>
#include <math.h>
#include <string>
namespace {
struct ConfigureReferenceCrt {
  ConfigureReferenceCrt() {
    const auto* requested = std::getenv("RESHIKI_REFERENCE_FMA3");
    if (!requested) return;
    const std::string mode(requested);
    if (mode != "0" && mode != "1") {
      std::fputs("Invalid reference FMA3 profile\n", stderr);
      std::exit(EXIT_FAILURE);
    }
    const int enabled = mode == "1" ? 1 : 0;
    if (_set_FMA3_enable(enabled) != enabled) {
      std::fputs("Requested reference FMA3 profile is unavailable\n", stderr);
      std::exit(EXIT_FAILURE);
    }
  }
};
const ConfigureReferenceCrt configure_reference_crt;
}
