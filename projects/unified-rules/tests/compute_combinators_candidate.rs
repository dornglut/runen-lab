//! Composability probe: function injection and relational operators are
//! optional, test-local helpers inside ONE scoped compute registration API.
//! They are not separate accepted frontends or indexed query implementations.
use std::collections::BTreeMap;
use unified_rules::{Each, Engine, Error, EventOrigin, Key, One, RuleCx};

fn required2<A, B, O, F>(
    function: F,
) -> impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, Key) -> Result<Option<O>, Error> + 'static
where
    A: Clone + Eq + 'static,
    B: Clone + Eq + 'static,
    O: Clone + Eq + 'static,
    F: Fn(&A, &B) -> Result<O, Error> + 'static,
{
    move |cx, key| {
        let Some(a) = cx.get::<A>(key)? else {
            return Ok(None);
        };
        let Some(b) = cx.get::<B>(key)? else {
            return Ok(None);
        };
        Ok(Some(function(&a, &b)?))
    }
}

struct Relation<T> {
    rows: Vec<(Key, T)>,
}

impl<T> Relation<T> {
    fn from_rows(rows: Vec<(Key, T)>) -> Self {
        Self { rows }
    }

    fn join<U>(self, other: Relation<U>) -> Relation<(T, U)> {
        let mut right: BTreeMap<Key, U> = other.rows.into_iter().collect();
        Relation {
            rows: self
                .rows
                .into_iter()
                .filter_map(|(key, left)| right.remove(&key).map(|right| (key, (left, right))))
                .collect(),
        }
    }

    fn filter(self, predicate: impl Fn(&T) -> bool) -> Self {
        Self {
            rows: self
                .rows
                .into_iter()
                .filter(|(_, value)| predicate(value))
                .collect(),
        }
    }

    fn checked_sum(self, amount: impl Fn(&T) -> i64) -> Result<i64, Error> {
        self.rows
            .into_iter()
            .try_fold(0_i64, |total, (_, value)| {
                total.checked_add(amount(&value)).ok_or(Error::Overflow)
            })
    }

    fn group_sum(
        self,
        group: impl Fn(&T) -> Key,
        amount: impl Fn(&T) -> i64,
    ) -> Result<BTreeMap<Key, i64>, Error> {
        let mut totals = BTreeMap::<Key, i64>::new();
        for (_, value) in self.rows {
            let total = totals.entry(group(&value)).or_default();
            *total = total.checked_add(amount(&value)).ok_or(Error::Overflow)?;
        }
        Ok(totals)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Price(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Quantity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Line {
    invoice: Key,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Amount(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Invoice {
    due: u64,
    paid: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InvoiceTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OpenBalance(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Day(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Overdue(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reminder(Key);

#[test]
fn invoice_composes_injected_functions_and_relations() -> Result<(), Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Day>>((), |day| day.0)?;

    // Ordinary fallible function, inferred input types; the public registration
    // remains compute::<Each<_>>, not a competing derive method.
    app.compute::<Each<Amount>>(required2(|price: &Price, qty: &Quantity| {
        price.0
            .checked_mul(qty.0)
            .map(Amount)
            .ok_or(Error::Overflow)
    }))?;

    app.compute::<Each<InvoiceTotal>>(|cx, key| {
        if cx.get::<Invoice>(key)?.is_none() {
            return Ok(None);
        }
        let totals = Relation::from_rows(cx.all::<Line>()?)
            .join(Relation::from_rows(cx.all::<Amount>()?))
            .group_sum(|(line, _)| line.invoice, |(_, amount)| amount.0)?;
        Ok(Some(InvoiceTotal(totals.get(&key).copied().unwrap_or(0))))
    })?;

    app.compute::<One<OpenBalance>>(|cx, ()| {
        let total = Relation::from_rows(cx.all::<Invoice>()?)
            .join(Relation::from_rows(cx.all::<InvoiceTotal>()?))
            .filter(|(invoice, _)| !invoice.paid)
            .checked_sum(|(_, total)| total.0)?;
        Ok(Some(OpenBalance(total)))
    })?;

    app.compute::<Each<Overdue>>(|cx, key| {
        let Some(invoice) = cx.get::<Invoice>(key)? else {
            return Ok(None);
        };
        let Some(day) = cx.singleton::<Day>()? else {
            return Ok(None);
        };
        let overdue = !invoice.paid && day.0 > invoice.due;
        if overdue && cx.previous::<Overdue>(key) != Some(Overdue(true)) {
            cx.emit(Reminder(key))?;
            cx.request(Reminder(key))?;
        }
        Ok(Some(Overdue(overdue)))
    })?;

    let a = Key::new(0, 1);
    let b = Key::new(1, 1);
    let l1 = Key::new(2, 1);
    let l2 = Key::new(3, 1);
    app.transact(|tx| {
        for key in [a, b, l1, l2] {
            tx.spawn(key)?;
        }
        tx.put_singleton(Day(10))?;
        tx.put(
            a,
            Invoice {
                due: 11,
                paid: false,
            },
        )?;
        tx.put(
            b,
            Invoice {
                due: 14,
                paid: false,
            },
        )?;
        tx.put(l1, Line { invoice: a })?;
        tx.put(l2, Line { invoice: a })?;
        tx.put(l1, Price(10))?;
        tx.put(l1, Quantity(2))?;
        tx.put(l2, Price(4))?;
        tx.put(l2, Quantity(3))?;
        Ok(())
    })?;
    assert_eq!(app.get::<InvoiceTotal>(a), Some(InvoiceTotal(32)));
    assert_eq!(app.get::<InvoiceTotal>(b), Some(InvoiceTotal(0)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(32)));

    // A relationship update must invalidate grouped totals and the singleton.
    app.transact(|tx| tx.put(l2, Line { invoice: b }))?;
    assert_eq!(app.get::<InvoiceTotal>(a), Some(InvoiceTotal(20)));
    assert_eq!(app.get::<InvoiceTotal>(b), Some(InvoiceTotal(12)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(32)));

    // Checked injected function error must reject the entire transaction.
    assert_eq!(
        app.transact(|tx| tx.put(l1, Price(i64::MAX))),
        Err(Error::Overflow)
    );
    assert_eq!(app.get::<Amount>(l1), Some(Amount(20)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(32)));

    app.transact(|tx| tx.put_singleton(Day(12)))?;
    assert_eq!(app.get::<Overdue>(a), Some(Overdue(true)));
    assert_eq!(app.get::<Overdue>(b), Some(Overdue(false)));
    assert_eq!(app.occurrences::<Reminder>().len(), 1);
    assert_eq!(app.occurrences::<Reminder>()[0].source, EventOrigin::Key(a));
    assert_eq!(app.pending_effects::<Reminder>().len(), 1);
    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Reminder>().is_empty());
    assert_eq!(app.pending_effects::<Reminder>().len(), 1);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Step {
    epoch: u64,
    dt: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Seed(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Velocity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Origin(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Local(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Parent(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorldPosition(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SpeedSum(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TickMarker(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pulse(u64);

#[test]
fn game_composes_injected_functions_and_relations() -> Result<(), Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Step>>((), |step| step.epoch)?;

    app.compute::<Each<Position>>(|cx, key| {
        let Some(Seed(seed)) = cx.get::<Seed>(key)? else {
            return Ok(None);
        };
        let Some(Velocity(velocity)) = cx.get::<Velocity>(key)? else {
            return Ok(None);
        };
        let previous = cx.previous::<Position>(key);
        let before = previous.unwrap_or(Position(seed));
        let Some(step) = cx.advanced_in::<One<Step>>(())? else {
            return Ok(Some(before));
        };
        if previous.is_none() {
            return Ok(Some(before));
        }
        let delta = velocity.checked_mul(step.dt).ok_or(Error::Overflow)?;
        Ok(Some(Position(
            before.0.checked_add(delta).ok_or(Error::Overflow)?,
        )))
    })?;

    app.compute::<Each<Local>>(required2(|origin: &Origin, position: &Position| {
        origin
            .0
            .checked_add(position.0)
            .map(Local)
            .ok_or(Error::Overflow)
    }))?;

    app.compute::<Each<WorldPosition>>(|cx, key| {
        let Some(Local(local)) = cx.get::<Local>(key)? else {
            return Ok(None);
        };
        let world = if let Some(Parent(parent)) = cx.get::<Parent>(key)? {
            if !cx.exists(parent) {
                return Err(Error::InvalidRelationship);
            }
            let WorldPosition(ancestor) = cx
                .get::<WorldPosition>(parent)?
                .ok_or(Error::InvalidRelationship)?;
            ancestor.checked_add(local).ok_or(Error::Overflow)?
        } else {
            local
        };
        Ok(Some(WorldPosition(world)))
    })?;

    app.compute::<One<SpeedSum>>(|cx, ()| {
        let total = Relation::from_rows(cx.all::<Velocity>()?)
            .checked_sum(|velocity| velocity.0)?;
        Ok(Some(SpeedSum(total)))
    })?;

    app.compute::<One<TickMarker>>(|cx, ()| {
        if let Some(step) = cx.advanced_in::<One<Step>>(())? {
            cx.emit(Pulse(step.epoch))?;
            return Ok(Some(TickMarker(step.epoch)));
        }
        Ok(cx.previous_singleton::<TickMarker>())
    })?;

    let root = Key::new(0, 1);
    let child = Key::new(1, 1);
    app.transact(|tx| {
        tx.spawn(root)?;
        tx.spawn(child)?;
        tx.put_singleton(Step { epoch: 1, dt: 1 })?;
        tx.put(root, Seed(10))?;
        tx.put(root, Velocity(2))?;
        tx.put(root, Origin(5))?;
        tx.put(child, Seed(3))?;
        tx.put(child, Velocity(1))?;
        tx.put(child, Origin(4))?;
        tx.put(child, Parent(root))?;
        Ok(())
    })?;
    assert_eq!(app.get::<Position>(root), Some(Position(10)));
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(22)));
    assert_eq!(app.singleton::<SpeedSum>(), Some(SpeedSum(3)));
    assert_eq!(app.occurrences::<Pulse>()[0].source, EventOrigin::Global);

    app.transact(|tx| tx.put_singleton(Step { epoch: 2, dt: 1 }))?;
    assert_eq!(app.get::<Position>(root), Some(Position(12)));
    assert_eq!(app.get::<Position>(child), Some(Position(4)));
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(25)));

    // A non-tick source edit must update derived hierarchy, not advance motion.
    app.transact(|tx| tx.put(root, Origin(8)))?;
    assert_eq!(app.get::<Position>(root), Some(Position(12)));
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(28)));
    assert!(app.occurrences::<Pulse>().is_empty());

    // A dynamic cycle rejects the proposed source edit and all computed work.
    assert_eq!(
        app.transact(|tx| tx.put(root, Parent(child))),
        Err(Error::RuleCycle)
    );
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(28)));

    app.transact(|tx| tx.remove::<Parent>(child))?;
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(8)));
    assert_eq!(app.singleton::<SpeedSum>(), Some(SpeedSum(3)));
    Ok(())
}
