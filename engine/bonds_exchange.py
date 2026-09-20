"""Preserve bond appearance independently from RDKit's molecular graph."""
import copy

DISPLAY = {'Solid':'plain', 'Dash':'dashed', 'Dot':'dotted', 'Bold':'bold',
           'Hash':'hashed', 'WedgeBegin':'wedge', 'WedgeEnd':'wedge',
           'WedgedHashBegin':'hash', 'WedgedHashEnd':'hash',
           'HollowWedgeBegin':'hollow_wedge', 'HollowWedgeEnd':'hollow_wedge', 'Wavy':'wavy'}
EXPORT = {'plain':'Solid', 'dashed':'Dash', 'dotted':'Dot', 'bold':'Bold', 'hashed':'Hash',
          'wedge':'WedgeBegin', 'hash':'WedgedHashBegin', 'hollow_wedge':'HollowWedgeBegin', 'wavy':'Wavy'}


def chemistry_xml(root):
    result=copy.deepcopy(root)
    for bond in result.iter('b'):
        display=bond.get('Display','Solid')
        if display not in DISPLAY or bond.get('Display2','Solid') not in ('Solid','Dash','Bold','DottedHydrogen'):
            raise ValueError('This CDXML bond display is not supported yet')
        if bond.get('Display2')=='DottedHydrogen': bond.attrib.pop('Display2')
        # Parse topology first; restore unsupported chemical orders before sanitizing.
        if bond.get('Order') in ('hydrogen','1.5'):
            bond.set('Order','1')
        # RDKit does not recognize these equivalent stereochemical depictions.
        if display.startswith('HollowWedge'):
            bond.set('Display',display.replace('HollowWedge','Wedge'))
        elif display=='Hash':
            bond.set('Display','WedgedHashBegin')
        elif display=='Bold' and bond.get('Order','1')=='1':
            bond.set('Display','WedgeBegin')
    return result


def read_bonds(root, parts, scale, colors):
    fragments={el.get('id'):el for el in root.iter('fragment')}
    parents={child:parent for parent in root.iter() for child in parent}
    result=[]
    offset=0
    parsed=set()
    for mol in parts:
        fragment=fragments.get(mol.GetProp('CDX_FRAG_ID')) if mol.HasProp('CDX_FRAG_ID') else None
        if fragment is None:
            raise ValueError('Could not associate a CDXML molecular fragment')
        parsed.add(fragment)
        nodes={}
        for node in fragment.findall('n'):
            xy=[float(x) for x in node.get('p','').split()]
            if len(xy)!=2:
                raise ValueError('Missing CDXML atom position')
            matches=[]
            for atom in mol.GetAtoms():
                p=mol.GetConformer().GetAtomPosition(atom.GetIdx())
                if atom.GetAtomicNum()==int(node.get('Element','6')) and abs(p.x*28-xy[0]*scale)<0.02 and abs(-p.y*28-xy[1]*scale)<0.02:
                    matches.append(offset+atom.GetIdx()+1)
            if len(matches)!=1:
                raise ValueError('Could not safely associate CDXML bond appearance with its atoms')
            nodes[node.get('id')]=matches[0]
        for el in fragment.findall('b'):
            order={'1':1,'2':2,'3':3,'1.5':4,'hydrogen':0,'dative':5,'4':6}.get(el.get('Order','1'))
            if order==4 and (el.get('Display')=='Dash' or el.get('Display2')=='Dash'): order=7
            if order is None:
                raise ValueError('This CDXML bond order is not supported yet')
            display=el.get('Display','Solid')
            if el.get('Display2')=='DottedHydrogen':order=0
            elif order==1 and display=='Dash':order=5
            if order==0: display='Dot'
            attrs={};ancestors=[];current=el
            while current is not None:
                ancestors.append(current);current=parents.get(current)
            for ancestor in reversed(ancestors):attrs.update(ancestor.attrib)
            color=int(attrs.get('color','3'))
            if not 0<=color<len(colors):raise ValueError('Invalid CDXML bond color')
            position=el.get('DoublePosition','auto').lower()
            if position not in ('auto','left','right','center'):
                raise ValueError('Invalid CDXML double-bond position')
            a,b=nodes[el.get('B')],nodes[el.get('E')]
            if display.endswith('End'):
                a,b=b,a
                position={'left':'right','right':'left'}.get(position,position)
            secondary = None
            if order != 0:
                if order == 2 and display == 'Bold' and not el.get('Display2'):
                    # ChemDraw omits Display2 for a bold / solid double bond.
                    secondary = 'plain'
                elif el.get('Display2') and el.get('Display2') != display:
                    secondary = DISPLAY[el.get('Display2')]
            result.append(dict(a=a,b=b,order=order,display=DISPLAY[display],z_order=int(attrs.get("Z",0)),
                secondary_display=secondary,
                double_position=position,color=colors[color]))
        offset+=mol.GetNumAtoms()
    if any(fragment.findall('n') and fragment not in parsed for fragment in fragments.values()):
        raise ValueError('Could not parse every CDXML molecular fragment; import was cancelled')
    return result


def write_bond(bond,color_id):
    attrs={'Display':EXPORT[bond.get('display','plain')], 'color':color_id(bond.get('color',[0,0,0]))}
    if bond.get('secondary_display') is not None:
        attrs['Display2']=EXPORT[bond['secondary_display']]
    if bond.get('double_position','auto')!='auto' and bond['order'] in (2,7):
        attrs['DoublePosition']=bond['double_position'].capitalize()
    if bond['order']==0: attrs.update(Order='1',Display='Dash',Display2='DottedHydrogen')
    if bond['order']==5 and bond.get('display')=='dashed': attrs.update(Order='1',Display='Dash')
    if bond['order']==7 and bond.get('display')=='dashed': attrs['Display2']='Dash'
    return attrs


def write_crossings(doc, nodes, page, middle):
    """Encode crossing depth with standard bond Z order and crossing references."""
    atoms={a['id']:a['position'] for a in doc['atoms']}
    bonds=doc['bonds']
    crossings={i:[] for i in range(len(bonds))}
    segments=[]
    for i,bond in enumerate(bonds):
        a,b=atoms[bond['a']],atoms[bond['b']]
        segments.append((min(a['x'],b['x']),i,a,b))
    active=[]; budget=200000
    for minimum,i,a,b in sorted(segments):
        active=[item for item in active if max(item[1]['x'],item[2]['x'])>=minimum]
        for j,c,d in active:
            budget-=1
            if budget<0: break
            if {bonds[i]['a'],bonds[i]['b']} & {bonds[j]['a'],bonds[j]['b']}:continue
            ax,ay=b['x']-a['x'],b['y']-a['y']; bx,by=d['x']-c['x'],d['y']-c['y']
            det=ax*by-ay*bx
            if abs(det)<.001:continue
            dx,dy=c['x']-a['x'],c['y']-a['y']
            t=(dx*by-dy*bx)/det;u=(dx*ay-dy*ax)/det
            if .04<t<.96 and .04<u<.96:
                crossings[i].append(j);crossings[j].append(i)
        if budget<0:break
        active.append((i,a,b))
    if not any(crossings.values()):return
    ordered=sorted(range(len(bonds)),key=lambda i:(bonds[i].get('z_order',0),i))
    if middle+len(ordered)+len(doc.get('graphics',[]))>32760:
        raise ValueError('Too many bond layers for editable interchange')
    for element in page.iter():
        if int(element.get('Z',middle))>middle:element.set('Z',str(int(element.get('Z'))+len(ordered)))
    for rank,i in enumerate(ordered):
        nodes[i].set('Z',str(middle+rank))
        if crossings[i]:nodes[i].set('CrossingBonds',' '.join(nodes[j].get('id') for j in crossings[i]))
