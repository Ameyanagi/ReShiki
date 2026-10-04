#ifndef RSH_GEOMETRY_H
#define RSH_GEOMETRY_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
#define RSH_GEOMETRY_NOEXCEPT noexcept
extern "C" {
#else
#define RSH_GEOMETRY_NOEXCEPT
#endif

/* ABI 1. All indices refer to input order, never canonical atom order.
 * Chiral tags are RDKit CW=1 / CCW=2 relative to input bond insertion order.
 * Bond order uses RDKit SINGLE=1, DOUBLE=2, TRIPLE=3, AROMATIC=12.
 * Stereo uses NONE=0, ANY=1, Z=2, E=3, CIS=4, TRANS=5. Specified
 * double-bond stereo requires references adjacent to a and b respectively.
 * Absent references are both UINT32_MAX. Unsupported chemistry is rejected.
 */
typedef struct RshAtom {
  uint32_t atomic_number, isotope;
  int32_t charge;
  uint32_t explicit_h, no_implicit, aromatic, radical, chiral_tag;
} RshAtom;

typedef struct RshBond {
  uint32_t a, b, order, aromatic, stereo;
  uint32_t stereo_atoms[2];
} RshBond;

typedef struct RshGeometryRequest {
  uint32_t abi_version, operation, field, conformers;
  int32_t seed;
  uint32_t max_iterations;
  const RshAtom *atoms;
  size_t atom_count;
  const RshBond *bonds;
  size_t bond_count;
  /* Angstrom XYZ triples; coordinate_count is the number of triples.
   * Generate=0 accepts no coordinates or fixed atoms. Relax=1 and
   * Evaluate=2 require coordinates for every original and calculation H.
   * Evaluate rejects fixed atoms and returns the unconstrained gradient.
   * Field: MMFF94=0, MMFF94s=1, UFF=2. There is no implicit fallback.
   */
  const double *coordinates;
  size_t coordinate_count;
  const uint32_t *fixed_atoms;
  size_t fixed_atom_count;
} RshGeometryRequest;

typedef struct RshGeometryResponse {
  void *owner;
  const double *coordinates;
  const double *gradient;
  const uint32_t *hydrogen_parents;
  size_t atom_count, original_count;
  double initial_energy, energy;
  uint32_t converged, field;
  const char *diagnostics;
  const char *error;
} RshGeometryResponse;

/* The caller supplies a zero-initialized response and must free it after
 * either return code: 0 = success, 1 = failure. Storage belongs to owner;
 * original atoms occupy [0, original_count), with temporary Hs appended.
 * hydrogen_parents has atom_count-original_count entries, each identifying
 * the original atom bonded to the corresponding appended hydrogen.
 * Evaluate returns all-atom gradients in kcal/(mol Angstrom); other
 * operations return a null gradient. Energies are kcal/mol. Free accepts
 * nullptr, releases ownership once, and resets the complete response.
 */
int32_t rsh_geometry_solve(const RshGeometryRequest *request,
                           RshGeometryResponse *response)
    RSH_GEOMETRY_NOEXCEPT;
void rsh_geometry_free(RshGeometryResponse *response) RSH_GEOMETRY_NOEXCEPT;

#ifdef __cplusplus
}
#endif
#undef RSH_GEOMETRY_NOEXCEPT
#endif
