"""Atom-owned chemical symbols. CDXML's represent child links a mark to a node."""
import math
import xml.etree.ElementTree as ET


def read_marks(root, mol, base, scale, object_map):
    nodes={n.get('id'):n for n in root.iter('n')}
    for el in root.iter('graphic'):
        reps=el.findall('represent')
        if not reps: continue
        owners={rep.get('object') for rep in reps}
        if len(owners)!=1 or any(rep.get('attribute') not in ('Charge','Radical') for rep in reps):
            raise ValueError('Unsupported chemical-symbol attachment')
        node=nodes.get(next(iter(owners)))
        if node is None: raise ValueError('Chemical symbol refers to a missing atom')
        xy=[float(v)*scale for v in node.attrib['p'].split()]
        matches=[]
        for atom in mol.GetAtoms():
            p=mol.GetConformer().GetAtomPosition(atom.GetIdx())
            if atom.GetAtomicNum()==int(node.get('Element','6')) and abs(p.x*28-xy[0])<.02 and abs(-p.y*28-xy[1])<.02:matches.append(atom)
        if len(matches)!=1:raise ValueError('Could not safely associate a chemical symbol with its atom')
        atom=matches[0];identifier=atom.GetIdx()+1
        symbol=el.get('SymbolType')
        if symbol in ('Plus','Minus','CirclePlus','CircleMinus'):
            if not atom.GetFormalCharge():raise ValueError('A charge symbol refers to an uncharged atom')
            if (atom.GetFormalCharge()>0) != symbol.endswith('Plus'):
                raise ValueError('Charge-symbol sign conflicts with its atom')
            kind='circled_charge' if symbol.startswith('Circle') else 'charge'
        elif symbol=='Electron':
            if atom.GetNumRadicalElectrons()!=1:raise ValueError('Single-electron mark conflicts with atom radical count')
            kind='radical'
        elif symbol=='LonePair':kind='radical' if atom.GetNumRadicalElectrons()==2 else 'lone_pair'
        else:raise ValueError('This attached CDXML symbol is not supported yet')
        box=[float(v) for v in el.attrib['BoundingBox'].split()]
        if len(box)!=4 or not all(math.isfinite(v) for v in box):raise ValueError('Invalid chemical symbol coordinates')
        size=math.hypot(box[0]-box[2],box[1]-box[3])
        if symbol in ('Electron','LonePair'):size*=2
        if not .5<=size<=96:raise ValueError('Unsupported chemical symbol size')
        if any(key in el.attrib for key in ('color','LineWidth','BoldWidth')):
            raise ValueError('Per-symbol color/line overrides are not supported yet')
        mark=dict(kind=kind,offset={'x':box[0]*scale-xy[0],'y':box[1]*scale-xy[1]},
                  angle=math.degrees(math.atan2(box[1]-box[3],box[0]-box[2])),size_pt=size)
        entry=next((a for a in base['atoms'] if a['id']==identifier),None)
        if entry is None:entry={'id':identifier};base['atoms'].append(entry)
        entry.setdefault('marks',[]).append(mark)
        object_map[el]=[identifier]


def write_marks(fragment, atoms, atom_ids, position, scale, first_id):
    for atom in atoms:
        for mark in atom.get('marks',[]):
            kind=mark['kind'];charge=atom.get('charge',0);radical=atom.get('radical_electrons',0)
            if kind in ('charge','circled_charge'):
                if not charge:continue
                if abs(charge)!=1:raise ValueError('Positioned multiple-charge marks need native/SVG/PDF/PNG export')
                symbol=('Circle' if kind=='circled_charge' else '')+('Plus' if charge>0 else 'Minus');attribute='Charge'
            elif kind=='radical':
                if not radical:continue
                symbol='Electron' if radical==1 else 'LonePair';attribute='Radical'
            elif kind=='lone_pair':
                if atom['element']=='C':raise ValueError('ChemDraw interprets a carbon lone-pair mark as a diradical; use native/SVG/PDF/PNG')
                symbol='LonePair';attribute='Radical'
            else:raise ValueError('Lone-pair bars and combined radical-ion marks need native/SVG/PDF/PNG export')
            # CDXML marks inherit drawing color and width. Native marks inherit
            # the atom's text color; fail if these would diverge.
            if atom.get('text_style',{}).get('color',[0,0,0])!=[0,0,0]:raise ValueError('Colored atom marks need native/SVG/PDF/PNG export')
            label_size=atom.get('text_style',{}).get('size_pt',10)
            if math.hypot(mark['offset']['x'],mark['offset']['y'])*scale>label_size*1.05:
                raise ValueError('ChemDraw can detach distant atom marks on import; use native/SVG/PDF/PNG or move the mark closer to its atom')
            size=mark.get('size_pt') or label_size*.75
            if symbol in ('Electron','LonePair'):size/=2
            angle=math.radians(mark.get('angle',0))
            center={k:atom['position'][k]+mark['offset'][k] for k in ('x','y')}
            owner_distance=math.hypot(mark['offset']['x'],mark['offset']['y'])
            if any(other['id']!=atom['id'] and math.hypot(center['x']-other['position']['x'],center['y']-other['position']['y'])<=owner_distance for other in atoms):
                raise ValueError('CDXML mark attachment is ambiguous near another atom; use native/SVG/PDF/PNG or reposition it')
            end={'x':center['x']-math.cos(angle)*size/scale,'y':center['y']-math.sin(angle)*size/scale}
            el=ET.SubElement(fragment,'graphic',id=str(first_id),GraphicType='Symbol',SymbolType=symbol,BoundingBox=position(center)+' '+position(end))
            ET.SubElement(el,'represent',attribute=attribute,object=str(atom_ids[atom['id']]))
            first_id+=1
    return first_id
