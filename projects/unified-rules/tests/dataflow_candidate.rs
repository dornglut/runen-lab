//! A typed relational/dataflow *frontend* candidate for Lab #15 / Engineering #146.
//!
//! It is deliberately test-local and reuses the existing rule engine. The
//! algebra offers reusable typed joins and grouped reductions, NOT an indexed,
//! incremental dataflow runtime. Full-snapshot scans and a fabricated global
//! Key are explicitly retained as negative evidence, not accepted semantics.
use std::collections::BTreeMap;
use unified_rules::{Engine, Error, Key, OutputChange};

#[derive(Clone, Debug)]
struct Relation<T> {
    rows: Vec<(Key, T)>,
}

impl<T> Relation<T> {
    fn from_rows(rows: Vec<(Key, T)>) -> Self {
        Self { rows }
    }

    /// A typed equijoin on record identity. This is an ordered-map join
    /// recomputed from scratch, NOT an incremental materialized relation.
    fn join<U>(self, right: Relation<U>) -> Relation<(T, U)> {
        let other: BTreeMap<_, _> = right.rows.into_iter().collect();
        let rows = self
            .rows
            .into_iter()
            .filter_map(|(key, left)| other.get(&key).map(|_| (key, left)))
            .collect::<Vec<_>>();
        // Avoid cloning right-hand values and do not assume U: Clone.
        let mut left: BTreeMap<_, _> = rows.into_iter().collect();
        Relation {
            rows: other
                .into_iter()
                .filter_map(|(key, right)| left.remove(&key).map(|left| (key, (left, right))))
                .collect(),
        }
    }

    /// A checked numeric group-reduction over arbitrary relationship keys.
    /// A group with no members is absent; the calling rule must define how
    /// that absence maps to its output.
    fn group_sum(
        self,
        group: impl Fn(&Key, &T) -> Key,
        amount: impl Fn(&T) -> i64,
    ) -> Result<BTreeMap<Key, i64>, Error> {
        let mut totals = BTreeMap::<Key, i64>::new();
        for (key, value) in self.rows {
            let bucket = group(&key, &value);
            let total = totals.entry(bucket).or_insert(0);
            *total = total.checked_add(amount(&value)).ok_or(Error::Overflow)?;
        }
        Ok(totals)
    }

    fn sum_by(self, amount: impl Fn(&T) -> i64) -> Result<i64, Error> {
        self.rows.into_iter().try_fold(0_i64, |total, (_, value)| {
            total.checked_add(amount(&value)).ok_or(Error::Overflow)
        })
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Invoice {
    due: u64,
    paid: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Line {
    invoice: Key,
    price: i64,
    quantity: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Day(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InvoiceTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Outstanding(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Overdue(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reminder(Key);

const CLOCK: Key = Key::new(0, 1);
const SUMMARY: Key = Key::new(1, 1);
const INVOICE_A: Key = Key::new(2, 1);
const INVOICE_B: Key = Key::new(3, 1);
const FIRST: Key = Key::new(4, 1);
const SECOND: Key = Key::new(5, 1);
const THIRD: Key = Key::new(6, 1);

fn build() -> Result<Engine, Error> {
    let mut model = Engine::new();
    model.clock::<Day>(CLOCK, |d| d.0)?;
    model.rule::<LineTotal>(|cx, key| {
        cx.get::<Line>(key)?
            .map(|line| {
                line.price
                    .checked_mul(line.quantity)
                    .map(LineTotal)
                    .ok_or(Error::Overflow)
            })
            .transpose()
    })?;
    model.rule::<InvoiceTotal>(|cx, key| {
        if cx.get::<Invoice>(key)?.is_none() {
            return Ok(None);
        }
        let joined = Relation::from_rows(cx.all::<Line>()?)
            .join(Relation::from_rows(cx.all::<LineTotal>()?));
        let totals = joined.group_sum(|_, (line, _)| line.invoice, |(_, total)| total.0)?;
        Ok(Some(InvoiceTotal(totals.get(&key).copied().unwrap_or(0))))
    })?;
    model.rule::<Outstanding>(|cx, key| {
        // Negative finding: the underlying engine has no global result scope.
        if key != SUMMARY {
            return Ok(None);
        }
        let joined = Relation::from_rows(cx.all::<Invoice>()?)
            .join(Relation::from_rows(cx.all::<InvoiceTotal>()?))
            .filter(|(invoice, _)| !invoice.paid);
        Ok(Some(Outstanding(joined.sum_by(|(_, total)| total.0)?)))
    })?;
    model.rule::<Overdue>(|cx, key| {
        let Some(invoice) = cx.get::<Invoice>(key)? else {
            return Ok(None);
        };
        let Some(day) = cx.get::<Day>(CLOCK)? else {
            return Ok(None);
        };
        let overdue = !invoice.paid && day.0 > invoice.due;
        if overdue && cx.previous::<Overdue>(key) != Some(Overdue(true)) {
            cx.emit(Reminder(key))?;
        }
        Ok(Some(Overdue(overdue)))
    })?;
    Ok(model)
}

fn invoice_fixture(model: &mut Engine) -> Result<(), Error> {
    model.transact(|tx| {
        for id in [CLOCK, SUMMARY, INVOICE_A, INVOICE_B, FIRST, SECOND, THIRD] {
            tx.spawn(id)?;
        }
        tx.put(CLOCK, Day(10))?;
        tx.put(
            INVOICE_A,
            Invoice {
                due: 11,
                paid: false,
            },
        )?;
        tx.put(
            INVOICE_B,
            Invoice {
                due: 14,
                paid: false,
            },
        )?;
        tx.put(
            FIRST,
            Line {
                invoice: INVOICE_A,
                price: 10,
                quantity: 2,
            },
        )?;
        tx.put(
            SECOND,
            Line {
                invoice: INVOICE_A,
                price: 4,
                quantity: 3,
            },
        )?;
        tx.put(
            THIRD,
            Line {
                invoice: INVOICE_B,
                price: 6,
                quantity: 5,
            },
        )?;
        Ok(())
    })
}

#[test]
fn domain_neutral_relational_join_retraction_and_temporal_event() -> Result<(), Error> {
    let mut model = build()?;
    invoice_fixture(&mut model)?;
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_A), Some(InvoiceTotal(32)));
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_B), Some(InvoiceTotal(30)));
    assert_eq!(model.get::<Outstanding>(SUMMARY), Some(Outstanding(62)));
    assert!(model.events::<Reminder>().is_empty());

    // Changed member, reparented relationship, and group-membership replacement.
    model.transact(|tx| {
        tx.put(
            FIRST,
            Line {
                invoice: INVOICE_A,
                price: 10,
                quantity: 4,
            },
        )
    })?;
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_A), Some(InvoiceTotal(52)));
    assert_eq!(model.get::<Outstanding>(SUMMARY), Some(Outstanding(82)));

    model.transact(|tx| {
        tx.put(
            SECOND,
            Line {
                invoice: INVOICE_B,
                price: 4,
                quantity: 3,
            },
        )
    })?;
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_A), Some(InvoiceTotal(40)));
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_B), Some(InvoiceTotal(42)));
    assert_eq!(model.get::<Outstanding>(SUMMARY), Some(Outstanding(82)));

    // Time is admitted monotonically, and an occurrence is staged once.
    model.transact(|tx| tx.put(CLOCK, Day(12)))?;
    assert_eq!(model.get::<Overdue>(INVOICE_A), Some(Overdue(true)));
    assert_eq!(model.get::<Overdue>(INVOICE_B), Some(Overdue(false)));
    assert_eq!(model.events::<Reminder>().len(), 1);
    assert_eq!(model.events::<Reminder>()[0].value, Reminder(INVOICE_A));

    model.transact(|_| Ok(()))?;
    assert!(model.events::<Reminder>().is_empty());

    // A failed derived computation rejects the entire proposed batch.
    assert_eq!(
        model.transact(|tx| {
            tx.put(
                FIRST,
                Line {
                    invoice: INVOICE_A,
                    price: i64::MAX,
                    quantity: 4,
                },
            )
        }),
        Err(Error::Overflow)
    );
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_A), Some(InvoiceTotal(40)));
    assert_eq!(model.get::<Outstanding>(SUMMARY), Some(Outstanding(82)));

    model.transact(|tx| tx.remove::<Line>(THIRD))?;
    assert_eq!(model.get::<InvoiceTotal>(INVOICE_B), Some(InvoiceTotal(12)));
    assert_eq!(model.get::<Outstanding>(SUMMARY), Some(Outstanding(52)));
    assert_eq!(
        model.changes::<LineTotal>(),
        vec![OutputChange::Removed(THIRD)]
    );

    model.transact(|tx| {
        tx.put(
            INVOICE_A,
            Invoice {
                due: 11,
                paid: true,
            },
        )
    })?;
    assert_eq!(model.get::<Outstanding>(SUMMARY), Some(Outstanding(12)));
    assert_eq!(model.get::<Overdue>(INVOICE_A), Some(Overdue(false)));
    Ok(())
}

#[test]
fn algebra_is_deterministic_and_checks_group_overflow() -> Result<(), Error> {
    let rows = Relation::from_rows(vec![
        (FIRST, (INVOICE_A, 8_i64)),
        (SECOND, (INVOICE_A, 3_i64)),
        (THIRD, (INVOICE_B, 4_i64)),
    ]);
    let grouped = rows.group_sum(|_, (id, _)| *id, |(_, amount)| *amount)?;
    assert_eq!(grouped[&INVOICE_A], 11);
    assert_eq!(grouped[&INVOICE_B], 4);
    let overflow = Relation::from_rows(vec![
        (FIRST, (INVOICE_A, i64::MAX)),
        (SECOND, (INVOICE_A, 1)),
    ]);
    assert_eq!(
        overflow.group_sum(|_, (id, _)| *id, |(_, v)| *v),
        Err(Error::Overflow)
    );
    Ok(())
}
