"""Explicit nested CDXML fragments, including their real chemical definitions."""
import copy
import math
import xml.etree.ElementTree as ET
if __package__:
    from .abbreviations import PRESETS
else:
    from abbreviations import PRESETS


def write(root,doc,atom_ids,position,write_text,text_defaults):
    next_id=max((int(el.get('id','0')) for el in root.iter()),default=0)+1
    atoms={a['id']:a for a in doc['atoms']}
    for group in doc.get('abbreviations',[]):
        member_ids={str(atom_ids[i]) for i in group['members']}
        nodes={n.get('id'):n for n in root.iter('n')}
        parents={child:parent for parent in root.iter() for child in parent}
        anchor=nodes[str(atom_ids[group['anchor']])];parent=parents[anchor]
        if any(parents[nodes[i]] is not parent for i in member_ids):raise ValueError('Abbreviation crosses drawing fragments')
        if any(atoms[i].get('marks') or atoms[i].get('display',{}).get('number') for i in group['members']):
            raise ValueError('Expand abbreviations with attached marks or atom numbers before external editable export')
        internal=[b for b in parent.findall('b') if {b.get('B'),b.get('E')}<=member_ids]
        external=[b for b in parent.findall('b') if (b.get('B') in member_ids)!=(b.get('E') in member_ids)]
        if len(external)>1:raise ValueError('Multiple abbreviation attachments are not supported yet')
        outer=ET.SubElement(parent,'n',id=str(next_id),p=anchor.get('p'),NodeType='Fragment',Z=anchor.get('Z','0'));next_id+=1
        inner=ET.SubElement(outer,'fragment',id=str(next_id));next_id+=1
        for i in group['members']:
            node=nodes[str(atom_ids[i])];parent.remove(node);inner.append(node)
        for bond in internal:parent.remove(bond);inner.append(bond)
        faces_left=False
        if external:
            bond=external[0];side='B' if bond.get('B') in member_ids else 'E';other='E' if side=='B' else 'B'
            other_node=nodes.get(bond.get(other))
            if other_node is None:raise ValueError('Missing outside abbreviation atom')
            faces_left=float(other_node.get('p').split()[0])>float(outer.get('p').split()[0])+0.01
            connection=str(next_id);next_id+=1
            ET.SubElement(inner,'n',id=connection,p=other_node.get('p'),NodeType='ExternalConnectionPoint')
            ET.SubElement(inner,'b',id=str(next_id),B=connection,E=anchor.get('id'),Order=bond.get('Order','1'));next_id+=1
            inner.set('ConnectionOrder',connection)
            outer.set('BondOrdering',bond.get('id'))
            bond.set(side,outer.get('id'))
        # Formula labels store their chemical reading order. Right alignment
        # reverses their display in native readers; pre-reversing flips twice.
        label=group['label']
        style={**text_defaults,**(atoms[group['anchor']].get('text_style') or {}),'formula':True,'script':'normal'}
        write_text(outer,label,{'style':style},p=outer.get('p'),LabelAlignment='Right' if faces_left else 'Left')


def flatten(root):
    records=[]
    wrappers=[n for n in root.iter('n') if n.get('NodeType') in ('Fragment','Nickname')]
    for outer in wrappers:
        fragments=outer.findall('fragment');text=outer.find('t')
        if len(fragments)!=1 or text is None:raise ValueError('An abbreviation needs its explicit chemical definition')
        inner=fragments[0]
        if any(n.get('NodeType') in ('Fragment','Nickname') for n in inner.iter('n')):
            raise ValueError('Nested abbreviations must be expanded before import')
        if any(el.tag not in ('fragment','n','b','t','s') for el in inner.iter()):
            raise ValueError('Unsupported objects within an abbreviation definition')
        nodes={n.get('id'):n for n in inner.findall('n')}
        connections=[n for n in nodes.values() if n.get('NodeType')=='ExternalConnectionPoint']
        if len(connections)>1:raise ValueError('Multiple abbreviation attachments are not supported yet')
        parents={child:parent for parent in root.iter() for child in parent};parent=parents[outer]
        if parent.tag!='fragment':raise ValueError('Abbreviation outside a molecular fragment')
        external=[b for b in parent.findall('b') if outer.get('id') in (b.get('B'),b.get('E'))]
        if len(external)>1:raise ValueError('Multiple abbreviation bonds are not supported yet')
        target=tuple(map(float,outer.get('p','').split()))
        if len(target)!=2 or not all(math.isfinite(x) for x in target):raise ValueError('Invalid abbreviation position')
        connection_bond=None;connection=None
        if connections:
            connection=connections[0]
            matches=[b for b in inner.findall('b') if connection.get('id') in (b.get('B'),b.get('E'))]
            if len(matches)!=1:raise ValueError('Invalid abbreviation connection point')
            connection_bond=matches[0]
            anchor=nodes.get(connection_bond.get('E') if connection_bond.get('B')==connection.get('id') else connection_bond.get('B'))
            if external and connection_bond.get('Order','1')!=external[0].get('Order','1'):
                raise ValueError('Abbreviation attachment bond order conflicts with its definition')
        else:
            if external:raise ValueError('Missing abbreviation attachment point')
            anchor=next(iter(nodes.values()),None)
        if anchor is None:raise ValueError('Empty abbreviation definition')
        origin=tuple(map(float,anchor.get('p','').split()))
        if len(origin)!=2:raise ValueError('Missing abbreviation atom position')
        rotation=0.0;scale=1.0
        if connection is not None and external:
            source=tuple(map(float,connection.get('p','').split()))
            outside_id=external[0].get('E') if external[0].get('B')==outer.get('id') else external[0].get('B')
            outside=next((n for n in parent.findall('n') if n.get('id')==outside_id),None)
            if outside is None:raise ValueError('Missing abbreviation neighbor')
            dest=tuple(map(float,outside.get('p','').split()))
            if len(source)!=2 or len(dest)!=2:raise ValueError('Invalid abbreviation attachment geometry')
            source_length=math.dist(source,origin);dest_length=math.dist(dest,target)
            if source_length>1e-6 and dest_length>1e-6:
                rotation=math.atan2(dest[1]-target[1],dest[0]-target[0])-math.atan2(source[1]-origin[1],source[0]-origin[0])
                scale=dest_length/source_length
        c,s=math.cos(rotation),math.sin(rotation)
        for node in nodes.values():
            if node is connection:continue
            values=tuple(map(float,node.get('p','').split()))
            if len(values)!=2 or not all(math.isfinite(v) for v in values):raise ValueError('Invalid abbreviation atom position')
            x,y=(values[0]-origin[0])*scale,(values[1]-origin[1])*scale
            node.set('p',f'{target[0]+x*c-y*s:.8g} {target[1]+x*s+y*c:.8g}')
            # Label p is a rendering hint; atom p remains authoritative.
            for t in node.findall('t'):t.set('p',node.get('p'))
            parent.append(node)
        for bond in inner.findall('b'):
            if bond is not connection_bond:parent.append(bond)
        for bond in external:
            bond.set('B' if bond.get('B')==outer.get('id') else 'E',anchor.get('id'))
        # Apply the collapsed label's typography to its visible attachment atom.
        for t in anchor.findall('t'):anchor.remove(t)
        label_text=copy.deepcopy(text);label_text.set('p',anchor.get('p'));anchor.append(label_text)
        label=''.join(t.text or '' for t in text.findall('s'))
        reverse=''
        for name,(_,back) in PRESETS.items():
            if label==name:reverse=back;break
            if back and label==back:label,reverse=name,back;break
        records.append(dict(label=label,reverse_label=reverse,anchor=anchor.get('id'),members=[i for i,n in nodes.items() if n is not connection]))
        parent.remove(outer)
    return records


def read(records,root,mol,scale):
    nodes={n.get('id'):n for n in root.iter('n')}
    def identifier(xml_id):
        node=nodes[xml_id];x,y=map(float,node.get('p').split())
        candidates=[]
        for atom in mol.GetAtoms():
            p=mol.GetConformer().GetAtomPosition(atom.GetIdx())
            if atom.GetAtomicNum()==int(node.get('Element','6')) and abs(p.x*28-x*scale)<0.02 and abs(-p.y*28-y*scale)<0.02:
                candidates.append(atom.GetIdx()+1)
        if len(candidates)!=1:raise ValueError('Could not safely associate abbreviation atoms')
        return candidates[0]
    return [dict(label=g['label'],reverse_label=g['reverse_label'],anchor=identifier(g['anchor']),members=[identifier(i) for i in g['members']]) for g in records]
