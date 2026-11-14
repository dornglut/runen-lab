//! One Bevy App/World owns authoritative invoice inputs. The publication is derived.
use crate::model::{Invoice, Line, Projection, Rejection, Source, initial, project};
use bevy_app::App;
use bevy_ecs::prelude::*;
use std::collections::BTreeMap;

#[derive(Component)]
struct InvoiceRow(Invoice);
#[derive(Component)]
struct LineRow(Line);
#[derive(Resource)]
struct Day(u64);

pub struct Backend {
    app: App,
    invoices: BTreeMap<u32, Entity>,
    lines: BTreeMap<u32, Entity>,
    published: Projection,
    refreshes: usize,
}

impl Backend {
    pub fn new() -> Self {
        let mut app = App::new();
        let source = initial();
        app.world_mut().insert_resource(Day(source.day));
        let invoices = source
            .invoices
            .iter()
            .map(|(&id, &invoice)| (id, app.world_mut().spawn(InvoiceRow(invoice)).id()))
            .collect();
        let lines = source
            .lines
            .iter()
            .map(|(&id, &line)| (id, app.world_mut().spawn(LineRow(line)).id()))
            .collect();
        let published = project(&source).expect("valid initial state");
        Self {
            app,
            invoices,
            lines,
            published,
            refreshes: 1,
        }
    }

    pub fn source(&self) -> Source {
        let world = self.app.world();
        let invoices = self
            .invoices
            .iter()
            .map(|(&id, &entity)| (id, world.get::<InvoiceRow>(entity).expect("live invoice").0))
            .collect();
        let lines = self
            .lines
            .iter()
            .map(|(&id, &entity)| (id, world.get::<LineRow>(entity).expect("live line").0))
            .collect();
        Source {
            invoices,
            lines,
            day: world.resource::<Day>().0,
        }
    }

    pub fn computed(&self) -> Result<Projection, Rejection> {
        project(&self.source())
    }

    // Inputs are already preflighted; this method is not a generic transaction.
    pub fn commit(&mut self, previous: &Source, next: &Source) {
        for (&id, &invoice) in &next.invoices {
            if previous.invoices.get(&id) != Some(&invoice) {
                let entity = self.invoices[&id];
                self.app
                    .world_mut()
                    .entity_mut(entity)
                    .insert(InvoiceRow(invoice));
            }
        }
        for &id in previous.lines.keys() {
            if !next.lines.contains_key(&id) {
                let entity = self.lines.remove(&id).expect("mapped line");
                assert!(self.app.world_mut().despawn(entity));
            }
        }
        for (&id, &line) in &next.lines {
            if previous.lines.get(&id) != Some(&line) {
                if let Some(&entity) = self.lines.get(&id) {
                    self.app
                        .world_mut()
                        .entity_mut(entity)
                        .insert(LineRow(line));
                } else {
                    let entity = self.app.world_mut().spawn(LineRow(line)).id();
                    self.lines.insert(id, entity);
                }
            }
        }
        if next.day != previous.day {
            self.app.world_mut().insert_resource(Day(next.day));
        }
    }

    pub fn refresh(&mut self) -> Vec<&'static str> {
        let next = self
            .computed()
            .expect("admitted native World must be valid");
        let changes = self.published.changes(next);
        self.published = next;
        self.refreshes += 1;
        changes
    }

    pub fn published(&self) -> Projection {
        self.published
    }
    pub fn refreshes(&self) -> usize {
        self.refreshes
    }
}
