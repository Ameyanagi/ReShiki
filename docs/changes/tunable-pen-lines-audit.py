#!/usr/bin/env python3
"""Independent, read-only audit of final b630 Pen desktop saves and raw evidence.

No ReShiki imports, GUI, Cargo, native-file writes, image transforms, or publication.
The earlier historical audit is retained and read as evidence, not executed again.
Only this script's JSON receipt is written, after its assertions pass.
"""
import copy
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess

COORD = Path('/Users/ryuichi/dev/ReShiki-worktrees/.coordination')
PEN = Path('/Users/ryuichi/dev/ReShiki-worktrees/pen-lines')
FIXTURES = PEN / 'tests/fixtures/tunable-pen-lines-67'
OUTPUT = COORD / 'pen-final-desktop-audit.json'
BUILD_HEAD = '2b3fdb4043fb7f4315f5c116a93194652ca09d17'
MERGE_HEAD = 'e10c876faf1c24bc807fbd4caa8d2117286e8ea5'
BINARY_SHA = 'b630b1ff119a681d82b35f0a64c20cda92e364d11e70a71f816a8ee9a9e8ed6c'
ORIGINAL_SHA = 'bfdeffe7ac7b3d49441abe93309e7f575dba83c6cea8e954e6201f8063b43ba9'
SELECT_SHA = 'c1ba44b9eefb2b4f357982dc0d0cb8cd015b3f4acd0bc45a2625f0e995316803'
EDIT_SHA = 'f32b1ed0ee928fc1f356aed6f2adb143d213aba6e4b880dfde3f32b1ee027ad2'
NODE_DRAG_SHA = 'f7dd075ac8d3b651609dd46be6f7d5517e9627cf4cb72a57af6dd5bf1e767d34'
CONTROL_DRAG_SHA = '289302f38af932141712f127676cd365b83c427d9f87ce7a340bae05e515c201'
OLD_CHECKER_SHA = 'b3565762928157f92f7e4838582983083cf2499ff6bb3e6626c4085a703ed5c9'
OLD_RECEIPT_SHA = 'c11df5a227b0fa359b5ece3d679e79ef36c28c3018af9e976ecaa43dcf6d472d'
CAPTURES = {
    'pen-continue-explicit-edit-points-final-retained.jpg': '6f8d167c9a4293505d8c8f87de30c048ed9eb08f6f1b64eb7e07ebe5ba7c0160',
    'pen-continue-explicit-select-final-appended.jpg': '34a33c2174bab84020ab08efad1d3b5fc682a4fd77d2b730a831ca221cc718e6',
    'pen-continue-explicit-select-final-retained.jpg': '053f1b336ff42621ef6fc9f968fe52bdd2bdab9226d2c4907cf00d5ce96198d4',
    'pen-continue-select-final-retained.jpg': '65494777956155023fc224195c64e48d1454fcdd6a2dac435c4f457c90b9cda9',
    'pen-final-continued-clean.jpg': '2218e66f2efb32e9cb56e1be70dba016dd3ea8c1f362b4612cd11e557d14b1ce',
    'pen-final-five-nodes-fresh-reopened-controls.jpg': 'd474ea1f2a7f476151b4c033052a0331c53678e53c2d6303def1f4d40d9ad7b7',
    'pen-node-click-final-no-edit.jpg': '713935e506cdfde507825f16e91a1e73756e69ea1b22dec95721f8ca52cc7e57',
    'pen-node-offset-drag-final.jpg': '8fc2abccdd1dbae91e1160f919ec4c669f88c6aa4866f860fb2377ae93f56c43',
    'pen-tangent-offset-drag-final.jpg': '79478b0314b0279464644b5c2436a5522baa528a2e4490ac82bbbdada7ca1397',
}
INPUTS, CHECKS, NATIVE = {}, [], []


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, description):
    if not condition:
        raise AssertionError(description)
    CHECKS.append(description)


def read(path, expected=None):
    path = Path(path)
    data = path.read_bytes()
    hash_value = digest(data)
    require(str(path) not in INPUTS or INPUTS[str(path)]['sha256'] == hash_value,
            f'input stable during audit: {path}')
    if expected:
        require(hash_value == expected, f'protected original SHA-256 matches: {path}')
    INPUTS[str(path)] = {'sha256': hash_value, 'bytes': len(data)}
    return data


def native(name, expected, expected_sha, route):
    path = FIXTURES / name
    data = read(path, expected_sha)
    doc = json.loads(data)
    require(doc == expected, f'every native field matches independent expected document: {name}')
    require(doc['version'] == 20 and len(doc['graphics']) == 1 and doc['graphics'][0]['id'] == 1,
            f'native20 contains exactly original path1: {name}')
    require(not any(doc[k] for k in ('atoms', 'bonds', 'annotations', 'arrows', 'groups')),
            f'empty chemistry/annotation/arrow/group arrays: {name}')
    NATIVE.append({'path': str(path), 'sha256': expected_sha, 'content_mime': 'application/json',
                   'format': 'ReShiki native JSON v20', 'node_count': len(doc['graphics'][0]['path']),
                   'route': route})
    return data


def delta(before, after, path=()):
    if isinstance(before, dict) and isinstance(after, dict):
        require(before.keys() == after.keys(), f'exact same object keys: {path}')
        return [d for key in before for d in delta(before[key], after[key], path + (key,))]
    if isinstance(before, list) and isinstance(after, list):
        require(len(before) == len(after), f'exact same list length: {path}')
        return [d for i in range(len(before)) for d in delta(before[i], after[i], path + (i,))]
    return [] if before == after else [{'path': list(path), 'before': before, 'after': after,
                                       'difference': after - before}]


def jpeg_size(data):
    require(data[:2] == b'\xff\xd8', 'original capture JPEG signature')
    offset = 2
    sof = {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}
    while offset < len(data):
        require(data[offset] == 0xFF, 'valid JPEG marker')
        while data[offset] == 0xFF:
            offset += 1
        marker = data[offset]
        offset += 1
        if marker in {0x01, 0xD8} or 0xD0 <= marker <= 0xD7:
            continue
        require(marker not in {0xD9, 0xDA}, 'JPEG dimensions precede scan/end')
        length = int.from_bytes(data[offset:offset + 2], 'big')
        require(length >= 2 and offset + length <= len(data), 'bounded JPEG segment')
        if marker in sof:
            return [int.from_bytes(data[offset + 5:offset + 7], 'big'),
                    int.from_bytes(data[offset + 3:offset + 5], 'big')]
        offset += length
    raise AssertionError('JPEG SOF missing')


def commit_source_hashes(head, paths):
    requests = b''.join(f'{head}:{p}\n'.encode() for p in paths)
    process = subprocess.run(['git', 'cat-file', '--batch'], cwd=PEN, input=requests,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    offset, result = 0, {}
    for path in paths:
        newline = process.stdout.index(b'\n', offset)
        header = process.stdout[offset:newline].split()
        require(len(header) == 3 and header[1] == b'blob', f'immutable source blob: {head}:{path}')
        size = int(header[2])
        start = newline + 1
        result[path] = digest(process.stdout[start:start + size])
        require(process.stdout[start + size:start + size + 1] == b'\n', 'git blob boundary')
        offset = start + size + 1
    require(offset == len(process.stdout), 'git batch fully consumed')
    return result


def main():
    original = {
        'atom_labels': {'carbons': 'skeletal', 'hydrogens': True, 'stereo': False},
        'version': 20, 'atoms': [], 'bonds': [], 'annotations': [], 'arrows': [],
        'graphics': [{'id': 1, 'kind': 'path', 'origin': {'x': 0.0, 'y': 0.0},
            'axis_x': {'x': 1.0, 'y': 0.0}, 'axis_y': {'x': 0.0, 'y': 1.0},
            'style': {'stroke': [0, 0, 0], 'fill': None, 'width_pt': 0.6, 'pattern': 'solid'},
            'sides': 'both', 'phase': 'solid', 'phase_flipped': False, 'layer': -1,
            'path': [
                {'command': 'move', 'points': {'x': -242.0, 'y': -46.0}},
                {'command': 'cubic', 'points': [{'x': -192.5, 'y': -121.0},
                    {'x': -141.5, 'y': 29.0}, {'x': -92.0, 'y': -46.0}]},
                {'command': 'line', 'points': {'x': 58.0, 'y': -96.0}},
                {'command': 'cubic', 'points': [{'x': 99.66667, 'y': -62.666664},
                    {'x': 183.0, 'y': -96.0}, {'x': 183.0, 'y': 4.0}]}]}], 'groups': []}
    original_bytes = read(FIXTURES / 'pen-authored-four-nodes.rsk', ORIGINAL_SHA)
    require(json.loads(original_bytes) == original, 'original complete native dictionary independently specified')

    for name, route in (
        ('pen-node-click-final-no-edit.rsk', 'stationary endpoint click'),
        ('pen-tangent-click-final-no-edit.rsk', 'stationary incoming control click'),
        ('pen-continue-explicit-select-final-no-edit.rsk', 'explicit Select then Continue'),
        ('pen-continue-explicit-edit-points-final-no-edit.rsk', 'explicit Edit Points then Continue'),
        ('pen-continue-select-final-no-edit.rsk', 'PRIVATE retained Edit Points tool trial; filename is not a Select claim'),
    ):
        require(native(name, original, ORIGINAL_SHA, route) == original_bytes,
                f'no-edit save is exact original raw bytes: {name}')

    node = copy.deepcopy(original)
    node['graphics'][0]['path'][3]['points'][1] = {'x': 196.48618, 'y': -109.486176}
    node['graphics'][0]['path'][3]['points'][2] = {'x': 196.48618, 'y': -9.486179}
    control = copy.deepcopy(original)
    control['graphics'][0]['path'][3]['points'][1] = {'x': 196.48618, 'y': -109.486176}
    drag_receipts = []
    for label, expected, expected_sha, point_indices in (
        ('node', node, NODE_DRAG_SHA, (1, 2)), ('tangent', control, CONTROL_DRAG_SHA, (1,))):
        name = f'pen-{label}-offset-drag-final.rsk'
        dragged = native(name, expected, expected_sha, 'near-offset pointer drag')
        changes = delta(original, expected)
        require({tuple(d['path']) for d in changes} ==
                {('graphics', 0, 'path', 3, 'points', i, axis) for i in point_indices for axis in ('x', 'y')},
                f'{label} changes only intended point objects and scalar coordinates')
        # 148 is a rounded UI percentage, not an exact floating-point zoom.
        world_min, world_max = 40 / (2 * 1.485), 40 / (2 * 1.475)
        for i in point_indices:
            old = original['graphics'][0]['path'][3]['points'][i]
            new = expected['graphics'][0]['path'][3]['points'][i]
            dx, dy = new['x'] - old['x'], new['y'] - old['y']
            require(world_min <= dx <= world_max and world_min <= -dy <= world_max,
                    f'{label} point{i} world movement agrees with 40px at rounded148% zoom')
            require(abs(dx + dy) <= 5e-6, f'{label} point{i} x/y displacement agrees within f32 rounding')
            require(40 / 2.98 <= dx <= 40 / 2.96,
                    f'{label} point{i} agrees with independent 297±1px/100-world-unit handle separation')
            require(not (43 / (2 * 1.485) <= dx <= 43 / (2 * 1.475)),
                    f'{label} point{i} preserves 3px grab offset rather than snapping to absolute release')
        if label == 'node':
            vector = expected['graphics'][0]['path'][3]['points']
            require(vector[1]['x'] == vector[2]['x'] and abs(vector[1]['y'] - vector[2]['y'] + 100) < 5e-6,
                    'endpoint drag translates incoming control and preserves 100-unit tangent vector')
        else:
            require(expected['graphics'][0]['path'][3]['points'][2] == {'x': 183.0, 'y': 4.0},
                    'incoming-control drag leaves endpoint exact')
        require(native(f'pen-{label}-offset-drag-final-undo.rsk', original, ORIGINAL_SHA, 'ROOT one Undo') == original_bytes,
                f'{label} one-Undo save restores original raw bytes')
        require(native(f'pen-{label}-offset-drag-final-redo.rsk', expected, expected_sha, 'ROOT one Redo') == dragged,
                f'{label} one-Redo save restores drag raw bytes')
        drag_receipts.append({'label': label, 'scalar_changes': changes, 'point_objects_changed': len(point_indices),
                              'world_displacement_bounds_from_rounded_zoom': [world_min, world_max]})

    continued = {}
    for route, endpoint, expected_sha in (
        ('select', {'x': 236.90263, 'y': 3.8988686}, SELECT_SHA),
        ('edit-points', {'x': 230.15955, 'y': 30.871231}, EDIT_SHA)):
        expected = copy.deepcopy(original)
        expected['graphics'][0]['path'].append({'command': 'line', 'points': endpoint})
        name = f'pen-continue-explicit-{route}-final-appended.rsk'
        data = native(name, expected, expected_sha, f'explicit {route} Continue followed by blank click')
        require(expected['graphics'][0]['path'][:-1] == original['graphics'][0]['path'],
                f'explicit {route} Continue preserves all four original commands')
        require(native(f'pen-continue-explicit-{route}-final-undo.rsk', original, ORIGINAL_SHA, 'ROOT one Undo') == original_bytes,
                f'explicit {route} one Undo raw-equals original')
        require(native(f'pen-continue-explicit-{route}-final-redo.rsk', expected, expected_sha, 'ROOT one Redo') == data,
                f'explicit {route} one Redo raw-equals appended')
        continued[route] = {'expected': expected, 'bytes': data, 'sha256': expected_sha, 'only_append': expected['graphics'][0]['path'][-1]}
    require(native('pen-continue-retained-tool-final-appended.rsk', continued['select']['expected'], SELECT_SHA,
                   'PRIVATE retained Edit Points tool trial; not explicit Select evidence') == continued['select']['bytes'],
            'private retained-tool trial has same append geometry, independently labelled')
    require(native('pen-final-five-nodes-fresh-reopened.rsk', continued['edit-points']['expected'], EDIT_SHA,
                   'fresh final-b630 process reopen and Save As') == continued['edit-points']['bytes'],
            'fresh process reopened five-node file retains exact appended raw bytes')
    require(len(NATIVE) == 19, 'all19 final native desktop saves independently audited')
    require({p.name for p in FIXTURES.glob('*final*.rsk')} ==
            {Path(item['path']).name for item in NATIVE}, 'all final native saves covered without extra or missing files')

    source = json.loads(read(COORD / 'pen-handles-source-provenance.json'))
    app = json.loads(read(COORD / 'pen-handles-app-provenance.json'))
    merge = json.loads(read(COORD / 'pen-curves-reference-merge-provenance.json'))
    historical = json.loads(read(COORD / 'pen-continue-desktop-audit.json', OLD_RECEIPT_SHA))
    read(COORD / 'pen-continue-desktop-audit.py', OLD_CHECKER_SHA)
    require(historical['status'] == 'pass', 'historical Continue audit PASS reused, not rerun')
    require(len(source['protected_files']) == 58, 'historical protected list still contains exactly58 artifacts')
    for path, expected_sha in source['protected_files'].items():
        require(historical['input_files'][path]['sha256'] == expected_sha,
                f'old protected receipt matches current protected declaration: {path}')
        read(path, expected_sha)
    # A mutable final-build source manifest has been updated legitimately since the old audit.
    mutable_source_receipt = str(COORD / 'pen-handles-source-provenance.json')
    historical_retained = 0
    for path, receipt in historical['input_files'].items():
        if path != mutable_source_receipt:
            read(path, receipt['sha256'])
            historical_retained += 1
    require(historical_retained == 62, 'all62 other historical audit inputs retained; updated final source manifest explicitly excepted')

    require(app['head'] == BUILD_HEAD and source['head'] == BUILD_HEAD, 'final signed app and source receipt identify 2b3 build')
    require(app['signed_binary_sha256'] == BINARY_SHA and merge['compiled_app_signed_sha256'] == BINARY_SHA,
            'same b630 final signed binary identified before and after reference merge')
    app_path = Path(app['app'])
    require({p.relative_to(app_path).as_posix() for p in app_path.rglob('*') if p.is_file()} ==
            {item['path'] for item in app['app_files']} and len(app['app_files']) == 9,
            'exactly nine original final signed bundle files')
    for item in app['app_files']:
        read(app_path / item['path'], item['sha256'])
    read(app_path / 'Contents/MacOS/reshiki', BINARY_SHA)
    codesign = subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app_path)],
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    require(codesign.returncode == 0, 'preserved final b630 app passes deep strict codesign')
    build_hashes = commit_source_hashes(BUILD_HEAD, source['source_files'])
    require(build_hashes == source['source_files'] and len(build_hashes) == 1072,
            'all1072 recorded compiled inputs independently match immutable2b3 commit')
    current_hashes = commit_source_hashes(MERGE_HEAD, source['source_files'])
    changed_inputs = [p for p in build_hashes if build_hashes[p] != current_hashes[p]]
    require(changed_inputs == ['reference/native_aromatic.rs'],
            'current e10c has one changed recorded Rust input; no false all1072-current-equal claim')
    changed_files = subprocess.check_output(['git', 'diff', '--name-only', BUILD_HEAD, MERGE_HEAD], cwd=PEN, text=True).splitlines()
    require(changed_files == ['reference/native_aromatic.md', 'reference/native_aromatic.rs',
                             'tests/fixtures/aromatic-arrows-exchange-source.py.gz', 'tests/fixtures/aromatic-worker-source.py.gz'],
            'post-build merge changes only documented reference guard and two archived test sources')
    guard_diff = subprocess.check_output(['git', 'diff', '--unified=3', BUILD_HEAD, MERGE_HEAD, '--',
                                        'reference/native_aromatic.rs'], cwd=PEN, text=True)
    require('aromatic-arrows-exchange-source.py.gz' in guard_diff and 'aromatic-worker-source.py.gz' in guard_diff,
            'changed reference guard selects original archived capture sources')
    require(merge['head'] == MERGE_HEAD and merge['compiled_app_head'] == BUILD_HEAD,
            'post-merge provenance keeps executed2b3 separate from currente10c')

    capture_receipts = []
    for name, expected_sha in CAPTURES.items():
        path = COORD / 'evidence/after' / name
        dimensions = jpeg_size(read(path, expected_sha))
        require(dimensions == [2560, 1704], f'raw final capture2560x1704 unchanged: {name}')
        fresh = name == 'pen-final-five-nodes-fresh-reopened-controls.jpg'
        private = name == 'pen-continue-select-final-retained.jpg'
        capture_receipts.append({'path': str(path), 'sha256': expected_sha, 'mime': 'image/jpeg',
            'pixel_dimensions': dimensions, 'zoom_percent_ROOT': 136 if fresh else 148,
            'source_commit_ROOT': BUILD_HEAD, 'signed_binary_sha256_ROOT': BINARY_SHA,
            'private_retained_tool_trial': private, 'modified_or_reencoded_by_audit': False})
    require({p.name for p in (COORD / 'evidence/after').glob('pen-*final*.jpg')} == set(CAPTURES),
            'all9 raw final Pen captures covered, including private retained-tool trial')

    for path, receipt in list(INPUTS.items()):
        require(digest(Path(path).read_bytes()) == receipt['sha256'], f'input unchanged at audit completion: {path}')
    result = {
        'status': 'pass', 'audit_utc': datetime.now(timezone.utc).isoformat(),
        'checker': {'path': str(Path(__file__).resolve()), 'sha256': digest(Path(__file__).read_bytes()),
                    'implementation_imports': False, 'gui_runs': 0, 'cargo_runs': 0, 'model_requests': 0},
        'counts': {'final_native_saves': len(NATIVE), 'final_raw_captures': len(capture_receipts),
                   'historical_protected_artifacts': 58, 'other_historical_audit_inputs_unchanged': historical_retained,
                   'final_app_files': 9, 'compiled_source_blobs': 1072,
                   'current_compiled_inputs_unchanged': 1071, 'current_test_guard_inputs_changed': 1},
        'geometry': {'original_sha256': ORIGINAL_SHA, 'original_commands': ['move', 'cubic', 'line', 'cubic'],
            'graphics': 1, 'path_id': 1, 'original_nodes': 4, 'appended_nodes': 5,
            'stationary_node_and_control_clicks_raw_equal_original': True, 'offset_drags': drag_receipts,
            'explicit_select_only_append': continued['select']['only_append'],
            'explicit_edit_points_only_append': continued['edit-points']['only_append'],
            'all_retained_one_undo_and_redo_saves_raw_equal_expected': True,
            'fresh_reopened_save_raw_equal_explicit_edit_points_appended': True,
            'fresh_reopened_sha256': EDIT_SHA},
        'independent_drag_oracle': {
            'observed_rounded_zoom_percent': 148, 'actual_zoom_interval': '[147.5%,148.5%)',
            'node_drag_screen_press': [1643, 1137], 'node_drag_screen_release': [1683, 1097],
            'pointer_displacement_pixels': [40, -40], 'node_rounded_screen_position_before': [1640, 1140],
            'incoming_control_rounded_screen_position_before': [1640, 843],
            'control_endpoint_world_separation': 100, 'observed_screen_separation_pixels': 297,
            'pixel_measurement_uncertainty': 1, 'scalar_rounding_tolerance': 5e-6,
            'exact_float_zoom_available': False,
            'absolute_release_snap_rejected': True,
            'note': 'No exact zoom or narrow zoom interval inferred from output is used as an oracle.'},
        'app_provenance': {'path': str(app_path), 'compiled_source_commit': BUILD_HEAD,
            'signed_binary_sha256': BINARY_SHA, 'same_preserved_bundle': True,
            'codesign_deep_strict_exit_code': codesign.returncode,
            'codesign_stdout': codesign.stdout, 'codesign_stderr': codesign.stderr,
            'source_manifest_aggregate_from_preserved_receipt': source['source_sha256'],
            'immutable_build_source_blob_hashes_all_match': True,
            'current_branch_commit': MERGE_HEAD, 'current_changed_compiled_inputs': changed_inputs,
            'current_changed_files': changed_files, 'reference_guard_diff': guard_diff,
            'all_current1072_equal_claimed': False},
        'historical_audit': {'receipt': str(COORD / 'pen-continue-desktop-audit.json'),
            'receipt_sha256': OLD_RECEIPT_SHA, 'checker_sha256': OLD_CHECKER_SHA,
            'reused_pass': True, 'rerun': False, 'all58_protected_data_artifacts_unchanged': True,
            'mutable_metadata_exception': mutable_source_receipt,
            'metadata_exception_reason': 'Final-build source receipt legitimately updated22c→2b3; historical app/native/capture bytes remain exact.'},
        'native_saves': NATIVE, 'raw_captures': capture_receipts,
        'baseline_node_click': {'path': str(COORD / 'evidence/baseline/pen-node-click-creates-edit-before-fix.jpg'),
            'sha256': '1053fc8f61e2ed27a21450e5de77f58149483a62535f3d3e8fc4d7f20aadb407',
            'source_commit': '01b81dc83d054d80773bacea7cf58527d4982b08',
            'signed_binary_sha256': '9e5a517fca9f1d264967ef5da730cd0fd92107d2733ba84bc9a3a48a3abfc811',
            'historical_mutation_kept': True},
        'private_trial': {'native_no_edit': 'pen-continue-select-final-no-edit.rsk',
            'native_appended': 'pen-continue-retained-tool-final-appended.rsk',
            'capture': 'pen-continue-select-final-retained.jpg', 'actual_route_ROOT': 'retained Edit Points after New File/Open',
            'explicit_select_claim': False, 'publication_candidate': False},
        'ROOT_observations': {
            'source': 'Task instructions; this audit independently examines artifacts without replaying GUI.',
            'matched_conditions': '148% rounded UI,2560×1704,F8 off,JACS ACS,Arial10; fresh reopened control capture is136%,not matched148.',
            'stationary_click_history': 'ROOT observed Undo and Redo disabled after node and incoming-control stationary clicks.',
            'explicit_select': 'ROOT explicitly selected Select (AX value on), selected node3, Continue:4nodes Open Node1 default; blank(1800,1140) appended one line.',
            'explicit_edit_points': 'ROOT explicitly selected Select→node3→Edit Points→stationary node4→Continue:4nodes Open Node4 retained; blank(1780,1220) appended one line.',
            'fresh_reopen': 'ROOT quit and opened explicit EditPoints appended file in fresh finalb630; SaveAs exact native,Undo/Redo disabled; Fit136%,Select→node3→Edit Points showed5nodes Open controls; recovery dismissed in RAM only.'},
        'limitations': [
            'Native JSON does not encode tool, selection or Undo stack; saved one-Undo/Redo equality is independently verified, while one-step GUI history and action order are ROOT observations.',
            'Raw capture hashes,MIME and dimensions verify retained originals; this audit does not independently replay actions or visually inspect every UI assertion.',
            'Runtime source association follows ROOT process observations and preserved signed-build receipts; exact bundle bytes and immutable source hashes are independently verified without rebuilding.',
            'The first filename containing select-final is a private retained-EditPoints trial; explicit Select evidence uses only explicit-select-final names.',
            'The final compiled1072-input manifest belongs to2b3,beforee10c changed one reference guard; it is not relabelled as all-current1072-equal.'],
        'input_files': INPUTS, 'checks_passed': len(CHECKS), 'checks': CHECKS}
    OUTPUT.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'status': 'pass', 'checks_passed': len(CHECKS), 'input_files': len(INPUTS),
                      'counts': result['counts'], 'receipt': str(OUTPUT)}, indent=2))


if __name__ == '__main__':
    main()
