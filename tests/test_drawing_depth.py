import copy
import unittest
import xml.etree.ElementTree as ET
from engine.worker import handle


def call(operation, **kwargs):
    return handle(dict(protocol=1, operation=operation, **kwargs))


class DrawingDepthTests(unittest.TestCase):
    def test_crossing_depth_survives_native_interchange_and_cleanup(self):
        doc = call('import', format='smiles', text='CC.CC')['document']
        for atom, (x, y) in zip(doc['atoms'], [(-30, 0), (30, 0), (0, -30), (0, 30)]):
            atom['position'] = dict(x=x, y=y)
        doc['bonds'][0]['z_order'] = 2
        doc['bonds'][1]['z_order'] = -2
        for fmt in ('cdxml', 'cdx'):
            exported = call('export', document=doc, format=fmt)['output']
            if fmt == 'cdxml':
                root = ET.fromstring(exported)
                bonds = list(root.iter('b'))
                self.assertGreater(int(bonds[0].get('Z')), int(bonds[1].get('Z')))
                self.assertEqual(bonds[0].get('CrossingBonds'), bonds[1].get('id'))
            back = call('import', text=exported, format=fmt)['document']
            horizontal = next(b for b in back['bonds'] if abs(next(a for a in back['atoms'] if a['id']==b['a'])['position']['y'] - next(a for a in back['atoms'] if a['id']==b['b'])['position']['y']) < .01)
            vertical = next(b for b in back['bonds'] if b is not horizontal)
            self.assertGreater(horizontal['z_order'], vertical['z_order'])
        clean = call('clean', document=doc, selected_ids=[1,2], cleanup=dict(scope='selected_atoms', keep_orientation=True))['document']
        self.assertEqual([b['z_order'] for b in clean['bonds']], [2,-2])

    def test_elbow_is_editable_through_xml_and_binary_exchange(self):
        doc = call('import', format='smiles', text='CCO')['document']
        doc['arrows'] = [dict(id=20, kind='bent', start=dict(x=100,y=0), end=dict(x=200,y=0),control=dict(x=130,y=-50))]
        for fmt in ('cdxml','cdx'):
            with self.subTest(format=fmt):
                out=call('export',document=doc,format=fmt)['output']
                back=call('import',format=fmt,text=out)['document']
                arrow=back['arrows'][0]
                self.assertEqual(arrow['kind'],'bent')
                for key in ('start','end','control'):
                    for axis in ('x','y'):
                        # CDXML may shift the drawing origin; vectors must match.
                        self.assertAlmostEqual(arrow[key][axis]-arrow['start'][axis],doc['arrows'][0][key][axis]-100*(axis=='x'),places=2)
                again=call('export',document=back,format=fmt)
                self.assertTrue(again['output'])


if __name__=='__main__':unittest.main()
