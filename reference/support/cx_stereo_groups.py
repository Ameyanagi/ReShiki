"""#1006: compare distinct CX groups with an oracle free of RDKit's u32 key wrap."""

import re
from functools import cache

GROUP = re.compile(r"(a|[o&][0-9]*):([0-9]+(?:,[0-9]+)*)")
QUERY = re.compile(
    r'(?m)^M  V30 [0-9]+ DAT [0-9]+ ATOMS=\([^)]*\) QUERYTYPE=SMARTSQ QUERYOP="=" FIELDDATA="([^"\r\n]*)"$'
)
U32_MAX = 2**32 - 1


def renumber(text):
    """Only complete, stereo-group-only CX extensions with actual key collisions.

    Keep repeated (kind, id) pairs together. Replace only overflowing keys that
    collide, using unused positive small ids; return the inverse metadata map.
    """
    match = re.fullmatch(r"(\S+[ \t]+\|)([^|]*)(\|)", text)
    if match is None:
        return None
    groups = list(GROUP.finditer(match[2]))
    if ",".join(group[0] for group in groups) != match[2]:
        return None
    keys = []
    for group in groups:
        kind = {"a": 0, "o": 1, "&": 2}[group[1][0]]
        identity = int(group[1][1:] or "0")
        if identity > U32_MAX or any(int(atom) > U32_MAX for atom in group[2].split(",")):
            return None
        keys.append((kind, identity))
    distinct = set(keys)

    def wrapped(key):
        return (key[1] * 10 + key[0]) & U32_MAX

    colliding = {
        key
        for key in distinct
        if any(other != key and wrapped(other) == wrapped(key) for other in distinct)
    }
    if not colliding:
        return None
    used_ids = {identity for _, identity in distinct}
    used_keys = {wrapped(key) for key in distinct}
    replacements = {}
    candidate = 1
    for key in keys:
        kind, identity = key
        if key not in colliding or identity * 10 + kind <= U32_MAX or key in replacements:
            continue
        while candidate in used_ids or candidate * 10 + kind in used_keys:
            candidate += 1
        replacements[key] = candidate
        used_ids.add(candidate)
        used_keys.add(candidate * 10 + kind)
    rewritten = ",".join(
        (group[1][0] + str(replacements[key]) if key in replacements else group[1]) + ":" + group[2]
        for group, key in zip(groups, keys, strict=True)
    )
    return match[1] + rewritten + match[3], {
        (kind, new): old for (kind, old), new in replacements.items()
    }


@cache
def native_keys_wrap():
    """Whether this RDKit merges distinct groups whose u32 keys wrap together.

    RDKit 2026.09.1 reads group ids with `res = std::atoi(num.c_str());` in
    `read_int` (Code/GraphMol/SmilesParse/CXSmilesOps.cpp:127, rdkit#9273;
    2026.03.6 used `boost::lexical_cast<unsigned int>`). Out of int range, glibc
    and macOS truncate, so `o2147483649` stays 2147483649 as an unsigned id and
    `10 * group_id + type` (line 1395) wraps onto o1's key. The MSVC runtime
    saturates at INT_MAX: on Windows x64 every larger id reads as 2147483647 and
    never collides, so there is nothing for the renumbering to change.
    """
    from rdkit import Chem

    mol = Chem.MolFromSmiles("F[C@H](Cl)[C@@H](Br)I |o1:1,o2147483649:3|")
    return len(mol.GetStereoGroups()) == 1


def unwrapped_collision(text):
    """A wrapping collision, in the text or its SMARTSQ data, that this RDKit reads unwrapped.

    Expected difference, RDKit 2026.09.1 on Windows x64 only (see native_keys_wrap):
    the case is compared without renumbering and counted.
    """
    return not native_keys_wrap() and any(
        renumber(field) for field in (text, *(match[1] for match in QUERY.finditer(text)))
    )


def query_renumbering(text, native_success, read, rejection):
    """A SMARTSQ field whose renumbering changes native acceptance to rejection.

    RDKit ignores the invalid wrapped query, but reads the renumbered query and
    rejects it at the editable-chemistry boundary. Rust checks that both texts
    instead produce exactly its unsupported substance-group-query error.
    """
    if not native_success:
        return None
    for match in QUERY.finditer(text):
        if alternate := renumber(match[1]):
            rewritten = text[: match.start(1)] + alternate[0] + text[match.end(1) :]
            try:
                read(rewritten)
            except (ValueError, RuntimeError, OverflowError) as error:
                if str(error) == rejection:
                    return rewritten
    return None
