//! One accepted RunenECS World owns authoritative invoice inputs.
use crate::model::{Invoice, Line, Projection, Rejection, Source, initial, project};
use runen_ecs::prelude::*;
use std::collections::BTreeMap;

#[derive(Component)]
struct InvoiceRow(Invoice);
#[derive(Component)]
struct LineRow(Line);
#[derive(Resource)]
struct Day(u64);

pub struct Backend {
    world: World,
    invoices: BTreeMap<u32, Entity>,
    lines: BTreeMap<u32, Entity>,
    published: Projection,
    refreshes: usize,
}

impl Backend {
    pub fn new() -> Self {
        let mut world = World::new();
        let source = initial();
        world.insert_resource(Day(source.day));
        let invoices = source.invoices.iter().map(|(&id, &invoice)| {
            (id, world.spawn(InvoiceRow(invoice)).expect("spawn invoice"))
        }).collect();
        let lines = source.lines.iter().map(|(&id, &line)| {
            (id, world.spawn(LineRow(line)).expect("spawn line"))
        }).collect();
        let published = project(&source).expect("valid initial state");
        Self { world, invoices, lines, published, refreshes: 1 }
    }

    pub fn source(&self) -> Source {
        let invoices = self.invoices.iter().map(|(&id, &entity)| {
            (id, self.world.require::<InvoiceRow>(entity).expect("live invoice").0)
        }).collect();
        let lines = self.lines.iter().map(|(&id, &entity)| {
            (id, self.world.require::<LineRow>(entity).expect("live line").0)
        }).collect();
        Source {
            invoices,
            lines,
            day: self.world.resource::<Day>().expect("clock resource").0,
        }
    }

    pub fn computed(&self) -> Result<Projection, Rejection> {
        project(&self.source())
    }

    pub fn commit(&mut self, previous: &Source, next: &Source) {
        for (&id, &invoice) in &next.invoices {
            if previous.invoices.get(&id) != Some(&invoice) {
                self.world.insert(self.invoices[&id], InvoiceRow(invoice)).expect("valid invoice");
            }
        }
        for &id in previous.lines.keys() {
            if !next.lines.contains_key(&id) {
                let entity = self.lines.remove(&id).expect("mapped line");
                self.world.despawn(entity).expect("valid line");
            }
        }
        for (&id, &line) in &next.lines {
            if previous.lines.get(&id) != Some(&line) {
                if let Some(&entity) = self.lines.get(&id) {
                    self.world.insert(entity, LineRow(line)).expect("valid line");
                } else {
                    let entity = self.world.spawn(LineRow(line)).expect("spawn line");
                    self.lines.insert(id, entity);
                }
            }
        }
        if next.day != previous.day {
            self.world.insert_resource(Day(next.day));
        }
    }

    pub fn refresh(&mut self) -> Vec<&'static str> {
        let next = self.computed().expect("admitted native World must be valid");
        let changes = self.published.changes(next);
        self.published = next;
        self.refreshes += 1;
        changes
    }

    pub fn published(&self) -> Projection { self.published }
    pub fn refreshes(&self) -> usize { self.refreshes }
}
