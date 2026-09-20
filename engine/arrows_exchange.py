"""Editable CDXML arrow exchange, checked against ChemDraw-saved objects.

Circular/elliptical arcs and arbitrary splines are rejected rather than silently
converted to a different curve. Moruno's single quadratic Bézier arrows use the
CDXML curve object's exact cubic representation.
"""
import math
import xml.etree.ElementTree as ET

HEADS = {'None': 'none', 'Full': 'full', 'HalfLeft': 'left', 'HalfRight': 'right'}
SHAPES = {'Solid': 'solid', 'Hollow': 'hollow', 'Angle': 'open'}


def appearance(arrow):
    kind = arrow.get('kind', 'forward')
    s = dict(head='full', tail='none', shape='solid', color=[0, 0, 0], width_pt=0.6,
             pattern='solid', head_length_pt=3.0, head_width_pt=1.2,
             head_notch=0.0, equilibrium_ratio=1.0, gap_pt=2.0, no_go='none', dipole=False)
    if kind == 'equilibrium': s.update(head='left', tail='left', shape='open')
    if kind == 'resonance': s.update(tail='full', shape='open')
    if kind == 'retro': s.update(shape='open', head_width_pt=2.4, gap_pt=1.4)
    if kind == 'fishhook': s.update(head='left')
    if kind == 'dipole': s.update(dipole=True, shape='open')
    if kind == 'no_go': s.update(no_go='cross')
    s.update(arrow.get('style') or {})
    return s


def control(arrow):
    if arrow.get('control') is not None: return arrow['control']
    if arrow.get('kind') in ('curved', 'fishhook', 'bent'):
        a, b = arrow['start'], arrow['end']
        return {'x': (a['x']+b['x']-b['y']+a['y'])/2,
                'y': (a['y']+b['y']+b['x']-a['x'])/2}
    return None


def read_arrow(el, root, point, colors, identifier):
    if float(el.get('AngularSize', '0')) != 0:
        raise ValueError('Circular/elliptical CDXML arrows are not supported yet; use a Bézier arrow')
    if el.get('FillType', 'None') not in ('None', 'Unspecified') or float(el.get('FadePercent', '100')) != 100:
        raise ValueError('Filled or faded CDXML arrows are not supported yet')
    if el.get('LineType', 'Solid') not in ('Solid', 'Dashed', 'Bold'):
        raise ValueError('Unsupported CDXML arrow stroke')
    try:
        head = HEADS[el.get('ArrowheadHead', 'None')]
        tail = HEADS[el.get('ArrowheadTail', 'None')]
        shape = SHAPES[el.get('ArrowheadType', 'Solid')]
    except KeyError as error:
        raise ValueError('Unsupported CDXML arrowhead') from error
    color = int(el.get('color', root.get('color', '3')))
    if not 0 <= color < len(colors): raise ValueError('Invalid arrow color')
    length = float(el.get('HeadSize', '1000')) / 100
    width = float(el.get('ArrowheadWidth', '250')) / 100
    center = float(el.get('ArrowheadCenterSize', str(length*100))) / 100
    # The reference editor stores head sizes and shaft spacing in hundredths
    # of a point. LineWidth is already in points.
    bold = el.get('LineType') == 'Bold' or bool(int(el.get('CurveType', '0')) & 4)
    line = float(el.get('BoldWidth' if bold else 'LineWidth', root.get('BoldWidth' if bold else 'LineWidth', '2' if bold else '0.6')))
    gap = float(el.get('ArrowShaftSpacing', '0')) / 100
    no_go = el.get('NoGo', 'None').lower()
    if no_go not in ('none', 'cross', 'hash'): raise ValueError('Unsupported CDXML no-go mark')
    if length <= 0 or not 0 <= center <= length: raise ValueError('Unsupported arrowhead center size')
    s = appearance({})
    s.update(head=head, tail=tail, shape=shape, color=colors[color], width_pt=line,
             pattern='dashed' if el.get('LineType') == 'Dashed' else 'solid',
             head_length_pt=length, head_width_pt=width, head_notch=1-center/length,
             gap_pt=gap or 2., no_go=no_go, dipole=el.get('Dipole') == 'yes')
    for key, lo, hi in [('width_pt', .1, 12), ('head_length_pt', .5, 24), ('head_width_pt', .25, 16), ('head_notch', 0, .9), ('gap_pt', .5, 16)]:
        if not math.isfinite(s[key]) or not lo <= s[key] <= hi: raise ValueError(f'Unsupported CDXML arrow {key}')
    kind = 'equilibrium' if gap else ('resonance' if head != 'none' and tail != 'none' else 'forward')
    result = dict(id=identifier, kind=kind, style=s)
    if el.tag == 'curve':
        flags = int(el.get('CurveType', '0'))
        if flags & ~(2 | 4 | 8 | 16 | 32 | 64) or el.get('Closed') == 'yes' or gap:
            raise ValueError('This arrowed CDXML spline is not supported yet')
        values = el.get('CurvePoints', '').split()
        if len(values) == 18:
            points = [point(' '.join(values[i:i+2])) for i in range(0, 18, 2)]
            start, corner, end = points[1], points[4], points[7]
            def on_segment(p, a, b):
                dx, dy = b['x']-a['x'], b['y']-a['y']
                length = math.hypot(dx,dy)
                if length < 1e-6: return math.hypot(p['x']-a['x'],p['y']-a['y']) < .15
                cross = abs((p['x']-a['x'])*dy-(p['y']-a['y'])*dx)/length
                t = ((p['x']-a['x'])*dx+(p['y']-a['y'])*dy)/(length*length)
                return cross < .15 and 0 <= t <= 1
            if not all(on_segment(p,a,b) for p,a,b in [(points[2],start,corner),(points[3],start,corner),(points[5],corner,end),(points[6],corner,end)]):
                raise ValueError('This multi-segment curved arrow cannot be represented by an elbow')
            result.update(start=start,end=end,control=corner,kind='bent')
            if flags & 2: s['pattern']='dashed'
            return result
        if len(values) != 12: raise ValueError('Only quadratic and single-elbow arrow curves are supported')
        points = [point(' '.join(values[i:i+2])) for i in range(0, 12, 2)]
        start, a, b, end = points[1:5]
        c1 = {k: start[k]+1.5*(a[k]-start[k]) for k in ('x', 'y')}
        c2 = {k: end[k]+1.5*(b[k]-end[k]) for k in ('x', 'y')}
        if math.hypot(c1['x']-c2['x'], c1['y']-c2['y']) > .15:
            raise ValueError('This cubic arrow cannot be represented by a single quadratic bend')
        result.update(start=start, end=end, control={k:(c1[k]+c2[k])/2 for k in ('x','y')}, kind='fishhook' if head in ('left','right') else 'curved')
        if flags & 2: s['pattern'] = 'dashed'
    else:
        result.update(start=point(el.attrib['Tail3D']), end=point(el.attrib['Head3D']))
    return result


def write_arrow(parent, arrow, identifier, position, color_id):
    s = appearance(arrow)
    c = control(arrow)
    if s['pattern'] == 'dotted' or arrow.get('kind') == 'retro':
        raise ValueError('Dotted and retrosynthesis arrows need native/SVG/PDF/PNG export; CDXML support is not available yet')
    if arrow.get('kind') == 'equilibrium' and (c is not None or s['equilibrium_ratio'] != 1):
        raise ValueError('Bent or unequal equilibrium arrows need native/SVG/PDF/PNG export')
    if s['dipole'] and s['tail'] != 'none':
        raise ValueError('ChemDraw removes the tail head on dipole arrows; use native/SVG/PDF/PNG')
    if arrow.get('kind') == 'equilibrium' and (s['head'] not in ('left','right') or s['tail'] not in ('left','right')):
        raise ValueError('CDXML equilibrium exchange requires half heads; use native/SVG/PDF/PNG')
    if c is not None and s['shape'] != 'solid':
        raise ValueError('ChemDraw does not preserve angled or hollow heads on these Bézier curves; use native/SVG/PDF/PNG')
    if c is not None and (s['dipole'] or s['no_go'] != 'none' or (s['head'] == 'none' and s['tail'] == 'none')):
        raise ValueError('This curved arrow decoration needs native/SVG/PDF/PNG export')
    attrs = dict(id=str(identifier), ArrowheadHead={v:k for k,v in HEADS.items()}[s['head']],
                 ArrowheadTail={v:k for k,v in HEADS.items()}[s['tail']],
                 ArrowheadType={v:k for k,v in SHAPES.items()}[s['shape']],
                 HeadSize=str(round(s['head_length_pt']*100)),
                 ArrowheadCenterSize=str(round(s['head_length_pt']*(1-s['head_notch'])*100)),
                 ArrowheadWidth=str(round(s['head_width_pt']*100)),
                 LineWidth=str(s['width_pt']), LineType='Dashed' if s['pattern']=='dashed' else 'Solid',
                 FillType='None', color=color_id(s['color']))
    if s['no_go'] != 'none': attrs['NoGo'] = s['no_go'].capitalize()
    if s['dipole']: attrs['Dipole'] = 'yes'
    if arrow.get('kind') == 'equilibrium': attrs['ArrowShaftSpacing'] = str(round(s['gap_pt']*100))
    start, end = arrow['start'], arrow['end']
    if c is None:
        attrs.update(Tail3D=position(start)+' 0', Head3D=position(end)+' 0')
        return ET.SubElement(parent, 'arrow', **attrs)
    if arrow.get('kind') == 'bent':
        def lerp(a,b,t): return {k:a[k]+(b[k]-a[k])*t for k in ('x','y')}
        points=(start,start,lerp(start,c,1/3),lerp(start,c,2/3),c,lerp(c,end,1/3),lerp(c,end,2/3),end,end)
        attrs.update(CurvePoints=' '.join(position(p) for p in points), CurveType=str(2 if s['pattern']=='dashed' else 0), Closed='no')
        return ET.SubElement(parent, 'curve', **attrs)
    a = {k:start[k]+2*(c[k]-start[k])/3 for k in ('x','y')}
    b = {k:end[k]+2*(c[k]-end[k])/3 for k in ('x','y')}
    attrs.update(CurvePoints=' '.join(position(p) for p in (start,start,a,b,end,end)),
                 CurveType=str(2 if s['pattern']=='dashed' else 0), Closed='no')
    return ET.SubElement(parent, 'curve', **attrs)
