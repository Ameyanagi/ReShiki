"""#1046 oracle inputs: discard empty or coincident CX coordinate sections."""

import re
from functools import cached_property


def _text_end(text, start, delimiters):
    # CX numeric escapes contain no literal delimiter. Leave them encoded so
    # escaped parentheses/semicolons cannot become coordinate syntax here.
    while start < len(text) and text[start] not in delimiters:
        start += 1
    return start


def coordinate_sections(text):
    """Locate coordinate tuples, skipping the CX fields that contain free text.

    Called only for inputs RDKit has already accepted with conformers. This is
    lexical coverage, not another chemistry parser: RDKit supplies all positions
    and decides acceptance. Keep delimiters and the trailing name byte-for-byte.
    """
    pos = text.index("|") + 1
    while text[pos] != "|":
        if text[pos] == "$":
            pos = text.index("$", pos + 1) + 1
        elif text.startswith("atomProp:", pos):
            pos += len("atomProp:")
            while text[pos] not in "|,":
                if text[pos].isascii() and text[pos].isdigit():
                    pos = _text_end(text, pos, ".") + 1
                    end = _text_end(text, pos, ".")
                    if end != pos:
                        pos = _text_end(text, end + 1, ":|,")
                    else:
                        pos = end
                if text[pos] not in "|,":
                    pos += 1
        elif text.startswith("SgD:", pos):
            pos += 4
            while text[pos] in "0123456789,":
                pos += 1
            # RDKit skips this separator unchecked, including a literal '('.
            pos += 1
            for _ in range(5):
                if text[pos] != "|":
                    pos = _text_end(text, pos, ":") + 1
            if text[pos] == "(":
                pos = text.index(")", pos) + 1
        elif text.startswith("Sg:", pos):
            pos = _text_end(text, pos + 3, ":") + 1
            while text[pos] in "0123456789,":
                pos += 1
            if text[pos] == ":":
                pos = _text_end(text, pos + 1, ":|")
                if text[pos] == ":":
                    pos = _text_end(text, pos + 1, ":|,")
        elif text.startswith("LN:", pos):
            # The extended link-node endpoint separator is also unchecked.
            match = re.match(
                r"LN:(?:[0-9]+:[0-9]+\.[0-9]+(?:\.[0-9]+[\s\S][0-9]+)?,?)*", text[pos:]
            )
            pos += match.end()
        elif text[pos] in "ct":
            # Native stereo tags consume arbitrary spelling up to the colon.
            pos = text.index(":", pos) + 1
        elif text[pos] == "(":
            end = text.index(")", pos) + 1
            contents = text[pos + 1 : end - 1]
            # Escape terminators and a trailing record separator do not start
            # another atom. E.g. (1&#48;,0,;) covers exactly one position.
            records = re.sub(r"&#[0-9]*;", "x", contents)
            count = records.count(";") + bool(records and not records.endswith(";"))
            yield pos, end, count
            pos = end
        else:
            pos += 1


class CoordinateInput:
    def __init__(self, text):
        self.text = text

    @cached_property
    def sections(self):
        return list(coordinate_sections(self.text))

    def dropped(self, mol, atom_offset=0):
        if not mol.GetNumConformers():
            return []
        assert mol.GetNumConformers() == len(self.sections), "CX/oracle conformer count changed"
        dropped = []
        for index, (conf, (_, _, count)) in enumerate(
            zip(mol.GetConformers(), self.sections, strict=True)
        ):
            first = tuple(conf.GetAtomPosition(0))
            coincident = mol.GetNumAtoms() > 1 and all(
                all(a == b for a, b in zip(conf.GetAtomPosition(i), first, strict=True))
                for i in range(mol.GetNumAtoms())
            )
            if count <= atom_offset or coincident:
                dropped.append(index)
        return dropped

    def drop(self, mol, atom_offset=0):
        dropped = self.dropped(mol, atom_offset)
        conformers = [conf.GetId() for conf in mol.GetConformers()]
        for index in dropped:
            mol.RemoveConformer(conformers[index])
        return bool(dropped)

    def without(self, dropped):
        text = self.text
        for index in reversed(dropped):
            start, end, _ = self.sections[index]
            # A space keeps adjacent tokens separate without extending CX lists.
            text = text[:start] + " " + text[end:]
        return text
