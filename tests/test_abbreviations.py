import copy
import base64
import unittest
from pathlib import Path
from engine.worker import handle
from engine.abbreviations import PRESETS

def run(**kw):return handle(dict(protocol=1,**kw))
def imported(smiles):return run(operation='import',format='smiles',text=smiles)

class Abbreviations(unittest.TestCase):
    def test_endpoint_replacement_has_expected_chemical_identity(self):
        expected={'OMe':'CCOC','Ph':'CCc1ccccc1','Boc':'CCC(=O)OC(C)(C)C','Bn':'CCCc1ccccc1',
                  'Cbz':'CCC(=O)OCc1ccccc1','TMS':'CC[Si](C)(C)C','TBS':'CC[Si](C)(C)C(C)(C)C',
                  'NO2':'CC[N+](=O)[O-]','CN':'CCC#N','CO2H':'CCC(=O)O'}
        for label,smiles in expected.items():
            with self.subTest(label=label):
                d=imported('CCC')['document'];target=d['atoms'][-1]['id'];old=copy.deepcopy(d)
                result=run(operation='abbreviate',format='replace',document=d,selected_ids=[target],text=label)
                self.assertEqual(result['analysis']['inchikey'],imported(smiles)['analysis']['inchikey'])
                self.assertEqual(d,old)
                g=result['document']['abbreviations'][0];self.assertEqual(g['anchor'],target)
                self.assertEqual(next(a for a in result['document']['atoms'] if a['id']==target)['position'],d['atoms'][-1]['position'])

    def test_every_preset_checks_and_roundtrips(self):
        for label in PRESETS:
            with self.subTest(label=label):
                d=imported('CCC')['document']
                result=run(operation='abbreviate',format='replace',document=d,selected_ids=[d['atoms'][-1]['id']],text=label)
                output=run(operation='export',document=result['document'],format='cdx')['output']
                back=run(operation='import',format='cdx',text=output)
                self.assertEqual(back['analysis']['inchikey'],result['analysis']['inchikey'])
                self.assertEqual(back['document']['abbreviations'][0]['label'],label)

    def test_matching_preserves_all_atoms_and_respects_selection(self):
        original=imported('COc1ccc(NC(=O)OC(C)(C)C)cc1')
        d=original['document'];ids=[a['id'] for a in d['atoms']]
        result=run(operation='abbreviate',document=d,selected_ids=ids)
        self.assertEqual(result['analysis']['inchikey'],original['analysis']['inchikey'])
        self.assertEqual({g['label'] for g in result['document']['abbreviations']},{'Boc','OMe'})
        self.assertEqual(len(result['document']['atoms']),len(d['atoms']))
        whole=run(operation='abbreviate',format='find',document=d,selected_ids=[])
        self.assertEqual({g['label'] for g in whole['document']['abbreviations']},{'Boc','OMe'})
        self.assertEqual(whole['analysis']['inchikey'],original['analysis']['inchikey'])
        partial=run(operation='abbreviate',document=d,selected_ids=[ids[0]])
        self.assertEqual(partial['document']['abbreviations'],[])
        checked=run(operation='clean',document=result['document'])
        self.assertEqual(checked['document']['abbreviations'],result['document']['abbreviations'])

    def test_invalid_attachment_and_metadata_are_rejected(self):
        d=imported('CCC')['document']
        for selection in ([],[2],[1,3]):
            with self.subTest(selection=selection):
                with self.assertRaises(ValueError):run(operation='abbreviate',format='replace',document=d,selected_ids=selection,text='OMe')
        d['abbreviations']=[dict(label='X',reverse_label='',anchor=2,members=[2])]
        with self.assertRaises(ValueError):run(operation='analyze',document=d)
        d['abbreviations'][0].update(members=[2,3,99])
        with self.assertRaises(ValueError):run(operation='analyze',document=d)

    def test_native_binary_groups_keep_chemistry_and_labels(self):
        path=Path(__file__).parent/'fixtures/abbreviations-native.cdx'
        result=run(operation='import',format='cdx',text=base64.b64encode(path.read_bytes()).decode())
        self.assertEqual(result['analysis']['inchikey'],imported('COc1ccc(NC(=O)OC(C)(C)C)cc1')['analysis']['inchikey'])
        self.assertEqual({g['label'] for g in result['document']['abbreviations']},{'Boc','OMe'})
        self.assertEqual(len(result['document']['atoms']),16)

if __name__=='__main__':unittest.main()
