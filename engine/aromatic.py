"""Aromatic display toggles retain molecular identity and original coordinates."""
import copy
import math
import xml.etree.ElementTree as ET
from rdkit import Chem


def toggle(doc, selection, read, write):
    selected = set(selection or [])
    if not selected:
        raise ValueError('Select an aromatic ring to change its display')
    mol = read(doc)
    ids = [a['id'] for a in doc['atoms']]
    edges = {frozenset((b['a'], b['b'])): b for b in doc['bonds']}
    rings = [ring for ring in mol.GetRingInfo().AtomRings()
             if all(ids[i] in selected for i in ring)
             and all(mol.GetBondBetweenAtoms(a, b).GetIsAromatic()
                     for a, b in zip(ring, (*ring[1:], ring[0])))]
    if not rings:
        raise ValueError('Select all atoms of an aromatic ring; saturated rings keep their chemistry')
    keys = {frozenset((ids[a], ids[b])) for ring in rings
            for a, b in zip(ring, (*ring[1:], ring[0]))}
    if any(g['members'] and selected.intersection(g['members']) for g in doc.get('abbreviations', [])):
        raise ValueError('Expand the abbreviation before changing its ring display')
    show = not all(edges[key]['order'] == 4 for key in keys)
    result = copy.deepcopy(doc)
    kekule = Chem.Mol(mol)
    Chem.Kekulize(kekule, clearAromaticFlags=True)
    index = {id: i for i, id in enumerate(ids)}
    for b in result['bonds']:
        if frozenset((b['a'], b['b'])) in keys:
            b['order'] = 4 if show else int(kekule.GetBondBetweenAtoms(index[b['a']], index[b['b']]).GetBondTypeAsDouble())
            b['display'] = 'plain'
            b['secondary_display'] = None
            b['stereo'] = None
            b['stereo_atoms'] = []
    if show:
        ring_atoms = {id for key in keys for id in key}
        for a in result['atoms']:
            source = mol.GetAtomWithIdx(index[a['id']])
            if a['id'] in ring_atoms and source.GetNumExplicitHs():
                a['explicit_h'] = source.GetNumExplicitHs()
    checked = read(result)
    if Chem.MolToSmiles(checked) != Chem.MolToSmiles(mol):
        raise ValueError('Ring display would change the molecular identity')
    computed = write(checked, result)
    for a, c in zip(result['atoms'], computed['atoms']):
        a['label_h'] = c['label_h']
    return result, checked


def circles(doc, mol):
    ids = [a['id'] for a in doc['atoms']]
    edges = {frozenset((b['a'], b['b'])): b for b in doc['bonds']}
    result = []
    for ring in mol.GetRingInfo().AtomRings():
        bonds = [edges[frozenset((ids[a], ids[b]))]
                 for a, b in zip(ring, (*ring[1:], ring[0]))]
        if not all(b['order'] == 4 for b in bonds):
            continue
        points = [doc['atoms'][i]['position'] for i in ring]
        center = {k: sum(p[k] for p in points) / len(points) for k in ('x', 'y')}
        distances, lengths = [], []
        for a, z in zip(points, (*points[1:], points[0])):
            dx, dy = z['x']-a['x'], z['y']-a['y']
            length = math.hypot(dx, dy)
            if length < 0.1:
                break
            t = max(0, min(1, ((center['x']-a['x'])*dx+(center['y']-a['y'])*dy)/length**2))
            distances.append(math.hypot(center['x']-a['x']-t*dx, center['y']-a['y']-t*dy))
            lengths.append(length)
        if len(distances) != len(points):
            continue
        radius = min(distances) - sum(lengths)/len(lengths)*0.18
        if radius > 3.5:
            result.append((center, radius, bonds[0].get('color', [0,0,0])))
    return result


def write_circles(fragment, doc, mol, position, color_id, next_id):
    for center, radius, color in circles(doc, mol):
        major = dict(x=center['x']+radius, y=center['y'])
        minor = dict(x=center['x'], y=center['y']+radius)
        ET.SubElement(fragment, 'graphic', id=str(next_id), GraphicType='Oval', OvalType='Circle',
                      BoundingBox=position(major)+' '+position(center),
                      Center3D=position(center)+' 0', MajorAxisEnd3D=position(major)+' 0',
                      MinorAxisEnd3D=position(minor)+' 0', color=color_id(color))
        next_id += 1
    return next_id


def remove_owned_circles(root, mol, base, scale):
    """Recognize molecular circles, keeping independent graphic ovals intact."""
    conf = mol.GetConformer() if mol.GetNumConformers() else None
    if conf is None:
        return
    doc = dict(atoms=[dict(id=i+1,position=dict(x=conf.GetAtomPosition(i).x*28,
                                              y=-conf.GetAtomPosition(i).y*28)) for i in range(mol.GetNumAtoms())],
               bonds=base['bonds'])
    expected = circles(doc, mol)
    for fragment in root.iter('fragment'):
        for el in list(fragment.findall('graphic')):
            if el.get('GraphicType') != 'Oval' or el.get('OvalType') != 'Circle':
                continue
            try:
                center = [float(v)*scale for v in el.get('Center3D','').split()[:2]]
                major = [float(v)*scale for v in el.get('MajorAxisEnd3D','').split()[:2]]
                if len(center) != 2 or len(major) != 2:
                    continue
                radius = math.dist(center, major)
                if any(math.dist(center, (c['x'],c['y'])) < 0.25 and abs(radius-r) < max(1.,r*0.2)
                       for c,r,_ in expected):
                    fragment.remove(el)
            except (ValueError, OverflowError):
                continue
