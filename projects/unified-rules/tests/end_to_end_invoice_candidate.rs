//! End-to-end invoice application consuming the EXACT same experimental
//! Engine::compute::<Each<T> / One<T>> contract as the game consumers.
//! No fake global entity, custom evaluator, scheduler, or effect executor.
//! This is an architectural decision probe, not a selected public frontend.
use std::collections::BTreeMap;
use unified_rules::{Each, Engine, Error, EventOrigin, Key, One};

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
struct LineAmount(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InvoiceTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OpenBalance(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Overdue(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reminder {
    invoice: Key,
    day: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReminderRequest {
    invoice: Key,
    day: u64,
}

const A: Key = Key::new(0, 1);
const B: Key = Key::new(1, 1);
const C: Key = Key::new(2, 1);
const L1: Key = Key::new(3, 1);
const L2: Key = Key::new(4, 1);
const L3: Key = Key::new(5, 1);

fn invoice_engine() -> Result<Engine, Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Day>>((), |day| day.0)?;

    app.compute::<Each<LineAmount>>(|cx, key| {
        let Some(line) = cx.get::<Line>(key)? else {
            return Ok(None);
        };
        if cx.get::<Invoice>(line.invoice)?.is_none() {
            return Err(Error::InvalidRelationship);
        }
        let amount = line
            .price
            .checked_mul(line.quantity)
            .ok_or(Error::Overflow)?;
        Ok(Some(LineAmount(amount)))
    })?;

    app.compute::<Each<InvoiceTotal>>(|cx, key| {
        if cx.get::<Invoice>(key)?.is_none() {
            return Ok(None);
        }
        // Explicitly costly: full-snapshot join and re-aggregation PER
        // invoice; no indexed/incremental relation primitive exists.
        let amounts: BTreeMap<_, _> = cx.all::<LineAmount>()?.into_iter().collect();
        let mut total = 0_i64;
        for (line_key, line) in cx.all::<Line>()? {
            if line.invoice == key {
                let amount = amounts.get(&line_key).ok_or(Error::InvalidRelationship)?;
                total = total.checked_add(amount.0).ok_or(Error::Overflow)?;
            }
        }
        Ok(Some(InvoiceTotal(total)))
    })?;

    app.compute::<One<OpenBalance>>(|cx, ()| {
        let mut outstanding = 0_i64;
        for (key, invoice) in cx.all::<Invoice>()? {
            if !invoice.paid {
                let total = cx
                    .get::<InvoiceTotal>(key)?
                    .ok_or(Error::InvalidRelationship)?;
                outstanding = outstanding.checked_add(total.0).ok_or(Error::Overflow)?;
            }
        }
        Ok(Some(OpenBalance(outstanding)))
    })?;

    app.compute::<Each<Overdue>>(|cx, key| {
        let Some(invoice) = cx.get::<Invoice>(key)? else {
            return Ok(None);
        };
        let Some(Day(day)) = cx.singleton::<Day>()? else {
            return Ok(None);
        };
        let overdue = !invoice.paid && day > invoice.due;
        // Ordinary typed state transition, not an epoch-wide side-effect
        // guarantee. A previous false/absent value only stages one request.
        if overdue && cx.previous::<Overdue>(key) != Some(Overdue(true)) {
            cx.emit(Reminder { invoice: key, day })?;
            cx.request(ReminderRequest { invoice: key, day })?;
        }
        Ok(Some(Overdue(overdue)))
    })?;
    Ok(app)
}

fn seed(app: &mut Engine) -> Result<(), Error> {
    app.transact(|tx| {
        for key in [A, B, L1, L2, L3] {
            tx.spawn(key)?;
        }
        tx.put(
            A,
            Invoice {
                due: 11,
                paid: false,
            },
        )?;
        tx.put(
            B,
            Invoice {
                due: 14,
                paid: false,
            },
        )?;
        tx.put(
            L1,
            Line {
                invoice: A,
                price: 10,
                quantity: 2,
            },
        )?;
        tx.put(
            L2,
            Line {
                invoice: A,
                price: 4,
                quantity: 3,
            },
        )?;
        tx.put(
            L3,
            Line {
                invoice: B,
                price: 6,
                quantity: 5,
            },
        )?;
        tx.put_singleton(Day(10))
    })
}

#[test]
fn complete_invoice_workload_needs_no_global_entity_and_preserves_rollback() -> Result<(), Error> {
    let mut app = invoice_engine()?;
    seed(&mut app)?;
    assert_eq!(app.keys(), vec![A, B, L1, L2, L3]);
    assert_eq!(app.singleton::<Day>(), Some(Day(10)));
    assert_eq!(app.get::<InvoiceTotal>(A), Some(InvoiceTotal(32)));
    assert_eq!(app.get::<InvoiceTotal>(B), Some(InvoiceTotal(30)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(62)));
    assert!(app.occurrences::<Reminder>().is_empty());
    assert!(app.pending_effects::<ReminderRequest>().is_empty());

    app.transact(|tx| {
        tx.put(
            L1,
            Line {
                invoice: A,
                price: 10,
                quantity: 4,
            },
        )
    })?;
    assert_eq!(app.get::<InvoiceTotal>(A), Some(InvoiceTotal(52)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(82)));

    app.transact(|tx| {
        tx.put(
            L2,
            Line {
                invoice: B,
                price: 4,
                quantity: 3,
            },
        )
    })?;
    assert_eq!(app.get::<InvoiceTotal>(A), Some(InvoiceTotal(40)));
    assert_eq!(app.get::<InvoiceTotal>(B), Some(InvoiceTotal(42)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(82)));

    app.transact(|tx| tx.put_singleton(Day(12)))?;
    assert_eq!(app.get::<Overdue>(A), Some(Overdue(true)));
    assert_eq!(app.get::<Overdue>(B), Some(Overdue(false)));
    let reminder = app.occurrences::<Reminder>();
    assert_eq!(reminder.len(), 1);
    assert_eq!(reminder[0].source, EventOrigin::Key(A));
    assert_eq!(
        reminder[0].value,
        Reminder {
            invoice: A,
            day: 12
        }
    );
    let staged = app.pending_effects::<ReminderRequest>();
    assert_eq!(staged.len(), 1);
    assert_eq!(
        staged[0].value,
        ReminderRequest {
            invoice: A,
            day: 12
        }
    );
    assert_eq!(staged[0].origin, EventOrigin::Key(A));

    // A rejected line arithmetic edit does not change any published data,
    // and it cannot replace the last successful commit's event snapshot.
    assert_eq!(
        app.transact(|tx| tx.put(
            L1,
            Line {
                invoice: A,
                price: i64::MAX,
                quantity: 4
            }
        )),
        Err(Error::Overflow)
    );
    assert_eq!(app.get::<InvoiceTotal>(A), Some(InvoiceTotal(40)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(82)));
    assert_eq!(app.occurrences::<Reminder>(), reminder);
    assert_eq!(app.pending_effects::<ReminderRequest>(), staged);

    // Missing relationship authority rejects the complete transaction too.
    assert_eq!(
        app.transact(|tx| tx.put(
            L3,
            Line {
                invoice: Key::new(100, 1),
                price: 6,
                quantity: 5,
            }
        )),
        Err(Error::InvalidRelationship)
    );
    assert_eq!(app.get::<InvoiceTotal>(B), Some(InvoiceTotal(42)));

    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Reminder>().is_empty());
    assert_eq!(app.pending_effects::<ReminderRequest>(), staged);
    assert!(app.acknowledge_effect(staged[0].id));
    assert!(app.pending_effects::<ReminderRequest>().is_empty());

    app.transact(|tx| tx.put_singleton(Day(15)))?;
    assert_eq!(app.get::<Overdue>(B), Some(Overdue(true)));
    assert_eq!(
        app.occurrences::<Reminder>()[0].value,
        Reminder {
            invoice: B,
            day: 15
        }
    );
    assert_eq!(app.pending_effects::<ReminderRequest>().len(), 1);

    app.transact(|tx| {
        tx.put(
            A,
            Invoice {
                due: 11,
                paid: true,
            },
        )
    })?;
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(42)));
    assert_eq!(app.get::<Overdue>(A), Some(Overdue(false)));
    app.transact(|tx| tx.remove::<Line>(L3))?;
    assert_eq!(app.get::<InvoiceTotal>(B), Some(InvoiceTotal(12)));
    assert_eq!(app.singleton::<OpenBalance>(), Some(OpenBalance(12)));
    assert_eq!(
        app.transact(|tx| tx.put_singleton(OpenBalance(99))),
        Err(Error::DerivedIsReadOnly)
    );
    Ok(())
}

#[test]
fn grouped_and_global_amounts_match_independent_48_step_reference() -> Result<(), Error> {
    let mut app = invoice_engine()?;
    let invoices = [A, B, C];
    let mut invoice_states = BTreeMap::new();
    let mut lines = BTreeMap::new();
    let mut day = 1_u64;
    app.transact(|tx| {
        for (i, key) in invoices.iter().copied().enumerate() {
            tx.spawn(key)?;
            let value = Invoice {
                due: 6 + (i as u64) * 3,
                paid: false,
            };
            tx.put(key, value)?;
            invoice_states.insert(key, value);
        }
        for n in 0_u32..12 {
            let key = Key::new(n + 10, 1);
            let value = Line {
                invoice: invoices[n as usize % 3],
                price: i64::from(n + 1),
                quantity: 1,
            };
            tx.spawn(key)?;
            tx.put(key, value)?;
            lines.insert(key, value);
        }
        tx.put_singleton(Day(day))
    })?;

    for step in 1_u32..=48 {
        let key = Key::new(step % 12 + 10, 1);
        let new_line = if step % 11 == 0 {
            None
        } else {
            Some(Line {
                invoice: invoices[step as usize % 3],
                price: i64::from((step % 17) + 1),
                quantity: i64::from((step % 5) + 1),
            })
        };
        let changed_invoice = if step % 9 == 0 {
            let key = invoices[(step as usize / 9) % 3];
            let mut invoice = invoice_states[&key];
            invoice.paid = !invoice.paid;
            Some((key, invoice))
        } else {
            None
        };
        let next_day = if step % 6 == 0 { Some(day + 1) } else { None };

        app.transact(|tx| {
            match new_line {
                Some(line) => tx.put(key, line)?,
                None => tx.remove::<Line>(key)?,
            };
            if let Some((invoice_key, value)) = changed_invoice {
                tx.put(invoice_key, value)?;
            }
            if let Some(next) = next_day {
                tx.put_singleton(Day(next))?;
            }
            Ok(())
        })?;
        match new_line {
            Some(line) => {
                lines.insert(key, line);
            }
            None => {
                lines.remove(&key);
            }
        }
        if let Some((key, invoice)) = changed_invoice {
            invoice_states.insert(key, invoice);
        }
        if let Some(next) = next_day {
            day = next;
        }

        let mut outstanding = 0_i64;
        for (invoice_key, invoice) in &invoice_states {
            let expected: i64 = lines
                .values()
                .filter(|line| line.invoice == *invoice_key)
                .map(|line| line.price * line.quantity)
                .sum();
            assert_eq!(
                app.get::<InvoiceTotal>(*invoice_key),
                Some(InvoiceTotal(expected)),
                "invoice total mismatch at step {step}",
            );
            assert_eq!(
                app.get::<Overdue>(*invoice_key),
                Some(Overdue(!invoice.paid && day > invoice.due)),
                "overdue mismatch at step {step}",
            );
            if !invoice.paid {
                outstanding += expected;
            }
        }
        assert_eq!(
            app.singleton::<OpenBalance>(),
            Some(OpenBalance(outstanding)),
            "global result mismatch at step {step}",
        );
        assert_eq!(app.keys().len(), 15);
    }
    Ok(())
}
