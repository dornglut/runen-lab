"""Independent headless oracle for Runen Lab #9, no ECS/runtime implementation."""
from __future__ import annotations
import hashlib
import json
import struct
from dataclasses import dataclass
from pathlib import Path

I32_MIN = -(1 << 31)
I32_MAX = (1 << 31) - 1

class Rejected(ValueError):
    pass

@dataclass(frozen=True)
class Object:
    gen: int
    x: int
    y: int
    radius: int
    material: int

class Oracle:
    def __init__(self, objects: int = 5, materials: int = 4):
        self.versions = {slot: 1 for slot in range(objects)}
        self.objects = {
            slot: Object(1, slot * 10, slot * -7, slot % 4 + 1, slot % materials)
            for slot in range(objects)
        }
        self.materials = {m: 0x101010 + m * 0x222222 for m in range(materials)}
        self.epoch = 0
        self._validate_state(self.objects, self.materials)

    @staticmethod
    def _validate_state(objects, materials):
        for o in objects.values():
            if o.radius < 0 or o.material not in materials:
                raise Rejected('INVALID_OBJECT')
            if not all(I32_MIN <= coord <= I32_MAX for coord in (
                o.x - o.radius, o.x + o.radius, o.y - o.radius, o.y + o.radius
            )):
                raise Rejected('BOUNDS_OVERFLOW')

    @staticmethod
    def _packets(objects, materials):
        return {
            (slot, o.gen): (o.x-o.radius, o.y-o.radius, o.x+o.radius, o.y+o.radius, materials[o.material])
            for slot, o in objects.items()
        }

    def source_digest(self):
        h = hashlib.sha256()
        for material, color in sorted(self.materials.items()):
            h.update(struct.pack('>II', material, color))
        for slot, last_gen in sorted(self.versions.items()):
            h.update(struct.pack('>II', slot, last_gen))
        for slot, obj in sorted(self.objects.items()):
            h.update(struct.pack('>IIiiiI', slot, obj.gen, obj.x, obj.y,
                                 obj.radius, obj.material))
        return h.hexdigest()

    def digest(self):
        h = hashlib.sha256()
        for (slot, gen), values in sorted(self._packets(self.objects, self.materials).items()):
            h.update(struct.pack('>IIiiiiI', slot, gen, *values))
        return h.hexdigest()

    def apply(self, ops):
        old = self._packets(self.objects, self.materials)
        staged_objects = self.objects.copy()
        staged_materials = self.materials.copy()
        staged_versions = self.versions.copy()
        targets = set()
        try:
            for op in ops:
                kind = op['kind']
                if kind == 'recolor':
                    material = op['material']
                    target = ('mat', material)
                else:
                    slot = op['slot']
                    target = ('obj', slot)
                if target in targets:
                    raise Rejected('DUPLICATE_TARGET')
                targets.add(target)
                if kind == 'recolor':
                    if material not in staged_materials:
                        raise Rejected('UNKNOWN_MATERIAL')
                    color = op['color']
                    if not 0 <= color <= 0xffffff:
                        raise Rejected('INVALID_COLOR')
                    staged_materials[material] = color
                elif kind == 'spawn':
                    gen = op['gen']
                    if slot in staged_objects or gen != staged_versions.get(slot, 0) + 1:
                        raise Rejected('INVALID_SPAWN')
                    staged_versions[slot] = gen
                    staged_objects[slot] = Object(gen, op['x'], op['y'], op['radius'], op['material'])
                else:
                    cur = staged_objects.get(slot)
                    if cur is None or cur.gen != op['gen']:
                        raise Rejected('STALE_OR_UNKNOWN_OBJECT')
                    if kind == 'delete':
                        del staged_objects[slot]
                    elif kind == 'move':
                        staged_objects[slot] = Object(cur.gen, op['x'], op['y'], cur.radius, cur.material)
                    elif kind == 'radius':
                        staged_objects[slot] = Object(cur.gen, cur.x, cur.y, op['radius'], cur.material)
                    elif kind == 'reassign':
                        staged_objects[slot] = Object(cur.gen, cur.x, cur.y, cur.radius, op['material'])
                    else:
                        raise Rejected('UNKNOWN_OPERATION')
            self._validate_state(staged_objects, staged_materials)
        except Rejected as error:
            return {'accepted': False, 'error': str(error), 'epoch': self.epoch,
                    'digest': self.digest(), 'source_digest': self.source_digest(), 'changed': [], 'removed': []}
        self.objects, self.materials, self.versions = staged_objects, staged_materials, staged_versions
        self.epoch += 1
        new = self._packets(self.objects, self.materials)
        changed = [[*key] for key in sorted(new) if old.get(key) != new[key]]
        removed = [[*key] for key in sorted(old) if key not in new]
        return {'accepted': True, 'epoch': self.epoch, 'digest': self.digest(), 'source_digest': self.source_digest(),
                'changed': changed, 'removed': removed}


def scripted():
    return [
        [{'kind':'recolor','material':0,'color':0x00ff00}],
        [{'kind':'move','slot':0,'gen':1,'x':5,'y':0}],
        [{'kind':'move','slot':1,'gen':1,'x':12,'y':0},
         {'kind':'recolor','material':2,'color':0xffff00}],
        [{'kind':'recolor','material':0,'color':0x00ffff},
         {'kind':'radius','slot':1,'gen':1,'radius':-1}],
        [{'kind':'delete','slot':0,'gen':1}],
        [{'kind':'spawn','slot':0,'gen':2,'x':10,'y':11,'radius':3,'material':2}],
        [{'kind':'move','slot':0,'gen':1,'x':3,'y':4}],
        [{'kind':'reassign','slot':1,'gen':1,'material':2}],
        [{'kind':'radius','slot':1,'gen':1,'radius':1}],
        [{'kind':'move','slot':0,'gen':2,'x':I32_MAX,'y':0}],
        [{'kind':'move','slot':2,'gen':1,'x':0,'y':0},
         {'kind':'radius','slot':2,'gen':1,'radius':2}],
        [{'kind':'recolor','material':3,'color':0}],
    ]

class SplitMix64:
    def __init__(self, seed): self.state = seed
    def next(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & 0xffffffffffffffff
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & 0xffffffffffffffff
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & 0xffffffffffffffff
        return z ^ (z >> 31)
    def choose(self, max_exclusive): return self.next() % max_exclusive


def randomized(oracle, rng, length):
    for _ in range(length):
        n = rng.choose(20)
        slot = rng.choose(15)
        gen = oracle.versions.get(slot, 0)
        live = oracle.objects.get(slot)
        if n == 0 and live is not None:
            yield [{'kind':'delete','slot':slot,'gen':live.gen}]
        elif n == 1 and live is None:
            yield [{'kind':'spawn','slot':slot,'gen':gen+1,'x':int(rng.choose(201))-100,
                    'y':int(rng.choose(201))-100,'radius':rng.choose(8),'material':rng.choose(4)}]
        elif n < 5:
            yield [{'kind':'recolor','material':rng.choose(4),'color':rng.choose(0x1000000)}]
        elif n < 10:
            yield [{'kind':'move','slot':slot,'gen':gen if rng.choose(16) else gen-1,
                    'x':int(rng.choose(201))-100,'y':int(rng.choose(201))-100}]
        elif n < 14:
            yield [{'kind':'radius','slot':slot,'gen':gen,
                    'radius':-1 if rng.choose(13)==0 else rng.choose(9)}]
        elif n < 17:
            yield [{'kind':'reassign','slot':slot,'gen':gen,'material':rng.choose(4)}]
        else:
            yield [{'kind':'recolor','material':rng.choose(4),'color':rng.choose(0x1000000)},
                   {'kind':'move','slot':slot,'gen':gen,'x':int(rng.choose(201))-100,
                    'y':int(rng.choose(201))-100}]


def generate(path: Path):
    oracle = Oracle()
    rng = SplitMix64(0x17bb593c4466e1d7)
    cases = list(scripted())
    trace = []
    for ops in cases:
        trace.append({'ops':ops, 'expect':oracle.apply(ops)})
    for ops in randomized(oracle, rng, 3000):
        trace.append({'ops':ops, 'expect':oracle.apply(ops)})
    with path.open('w', encoding='utf8', newline='\n') as f:
        f.write(json.dumps({'header':{'format':'runen-lab-tde-oracle-v1', 'initial_objects':5,
                            'initial_materials':4, 'initial_digest':Oracle().digest(), 'initial_source_digest':Oracle().source_digest(),
                            'seed':'0x17bb593c4466e1d7'}},separators=(',',':'))+'\n')
        for row in trace: f.write(json.dumps(row,separators=(',',':'))+'\n')
    return trace


def verify(path: Path):
    oracle = Oracle()
    records = path.read_text(encoding='utf8').splitlines()
    header = json.loads(records[0])['header']
    assert header['initial_digest'] == oracle.digest()
    assert header['initial_source_digest'] == oracle.source_digest()
    for lineno, line in enumerate(records[1:], start=2):
        row = json.loads(line)
        result = oracle.apply(row['ops'])
        if result != row['expect']:
            raise AssertionError((lineno, result, row['expect']))
    return len(records)-1

if __name__ == '__main__':
    import sys
    path = Path(__file__).with_name('golden.jsonl')
    if len(sys.argv)>1 and sys.argv[1]=='--verify':
        print(f'verified {verify(path)} transactions')
    else:
        entries = generate(path)
        print(f'generated {len(entries)} transactions, {sum(r["expect"]["accepted"] for r in entries)} accepted, {sum(not r["expect"]["accepted"] for r in entries)} rejected')
        print(f'verified {verify(path)} transactions')
