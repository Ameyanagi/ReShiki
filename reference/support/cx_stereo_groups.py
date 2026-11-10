"""#1006: compare distinct CX groups with an oracle free of RDKit's u32 key wrap."""

import re

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
