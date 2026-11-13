#!/usr/bin/env python3
"""Independent algorithm-level check for the small generated keyed-join plan.

This does NOT execute the Rust source, establish compilation, or benchmark ECS.
"""
import json
from pathlib import Path
from oracle_reference import Oracle, verify

ROOT = Path(__file__).resolve().parents[1]


def packets(model):
    return model._packets(model.objects, model.materials)


def verify_join(path):
    model = Oracle()
    inverse = {}
    published = packets(model)
    for slot, obj in model.objects.items():
        inverse.setdefault(obj.material, set()).add((slot, obj.gen))
    with path.open() as stream:
        next(stream)
        for seq, line in enumerate(stream):
            case = json.loads(line)
            old_objects = model.objects.copy()
            old_inverse = {m: members.copy() for m, members in inverse.items()}
            old = published.copy()
            result = model.apply(case['ops'])
            assert result == case['expect'], (path, seq, 'oracle mismatch')
            if not result['accepted']:
                assert published == old and inverse == old_inverse
                continue
            touched_slots = {op['slot'] for op in case['ops'] if op['kind'] != 'recolor'}
            touched_materials = {op['material'] for op in case['ops'] if op['kind'] == 'recolor'}
            before_groups = {m: old_inverse.get(m, set()).copy() for m in touched_materials}
            dirty_keys = set()
            for slot in touched_slots:
                previous = old_objects.get(slot)
                following = model.objects.get(slot)
                if previous:
                    oldkey = (slot, previous.gen)
                    dirty_keys.add(oldkey)
                if following:
                    newkey = (slot, following.gen)
                    dirty_keys.add(newkey)
                # Same-key, same-material edits do not change the index.
                if previous and following and previous.gen == following.gen and previous.material == following.material:
                    continue
                if previous:
                    group = inverse[previous.material]
                    group.remove(oldkey)
                    if not group:
                        del inverse[previous.material]
                if following:
                    inverse.setdefault(following.material, set()).add(newkey)
            affected = dirty_keys
            for mat in touched_materials:
                affected |= before_groups.get(mat, set())
                affected |= inverse.get(mat, set())
            all_now = packets(model)
            for key in affected:
                if key in all_now:
                    published[key] = all_now[key]
                else:
                    published.pop(key, None)
            expected_index = {}
            for slot, obj in model.objects.items():
                expected_index.setdefault(obj.material, set()).add((slot, obj.gen))
            assert inverse == expected_index, (path, seq, 'inverse index divergence')
            assert published == all_now, (path, seq, 'packet divergence')
            assert sorted([list(k) for k in published if old.get(k) != published[k]]) == result['changed'], (path, seq, 'changes')
            assert sorted([list(k) for k in old if k not in published]) == result['removed'], (path, seq, 'removed')
    return seq + 1


if __name__ == '__main__':
    total = 0
    for path in sorted((ROOT/'fixtures').glob('*.jsonl')):
        n = verify(path)
        k = verify_join(path)
        assert n == k
        total += k
        print(f'{path.name}: oracle {n}, independent indexed-join algorithm {k} — PASS')
    print(f'total checked: {total}')
