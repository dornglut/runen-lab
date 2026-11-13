//! Research-only function-injection API candidate.
//!
//! This is an API ergonomics probe, NOT a new accepted framework surface.
//! It delegates to the existing generic rule engine and deliberately exposes
//! the limits of a purely per-key, infallible function contract.
use unified_rules::{Engine, Error, Key, OutputChange};

trait Inject<Inputs, Output> {
    fn install(self, engine: &mut Engine) -> Result<(), Error>;
}

impl<F, A, Output> Inject<(A,), Output> for F
where
    A: Clone + Eq + 'static,
    Output: Clone + Eq + 'static,
    F: Fn(&A) -> Output + 'static,
{
    fn install(self, engine: &mut Engine) -> Result<(), Error> {
        engine.rule::<Output>(move |ctx, key| {
            Ok(ctx.get::<A>(key)?.map(|a| self(&a)))
        })
    }
}

impl<F, A, B, Output> Inject<(A, B), Output> for F
where
    A: Clone + Eq + 'static,
    B: Clone + Eq + 'static,
    Output: Clone + Eq + 'static,
    F: Fn(&A, &B) -> Output + 'static,
{
    fn install(self, engine: &mut Engine) -> Result<(), Error> {
        engine.rule::<Output>(move |ctx, key| {
            let Some(a) = ctx.get::<A>(key)? else {
                return Ok(None);
            };
            let Some(b) = ctx.get::<B>(key)? else {
                return Ok(None);
            };
            Ok(Some(self(&a, &b)))
        })
    }
}

impl<F, A, B, C, Output> Inject<(A, B, C), Output> for F
where
    A: Clone + Eq + 'static,
    B: Clone + Eq + 'static,
    C: Clone + Eq + 'static,
    Output: Clone + Eq + 'static,
    F: Fn(&A, &B, &C) -> Output + 'static,
{
    fn install(self, engine: &mut Engine) -> Result<(), Error> {
        engine.rule::<Output>(move |ctx, key| {
            let Some(a) = ctx.get::<A>(key)? else {
                return Ok(None);
            };
            let Some(b) = ctx.get::<B>(key)? else {
                return Ok(None);
            };
            let Some(c) = ctx.get::<C>(key)? else {
                return Ok(None);
            };
            Ok(Some(self(&a, &b, &c)))
        })
    }
}

trait FunctionCandidate {
    fn derive<Inputs, Output, F>(&mut self, rule: F) -> Result<(), Error>
    where
        F: Inject<Inputs, Output>;
}

impl FunctionCandidate for Engine {
    fn derive<Inputs, Output, F>(&mut self, rule: F) -> Result<(), Error>
    where
        F: Inject<Inputs, Output>,
    {
        rule.install(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnitPrice(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Quantity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Shipping(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineSubtotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InvoicedTotal(i64);

#[test]
fn ordinary_functions_derive_invoice_values_with_retraction() -> Result<(), Error> {
    let mut application = Engine::new();

    // No manual query loop, entity-coupled type name, or rule-context bookkeeping.
    application.derive(|price: &UnitPrice, quantity: &Quantity| {
        LineSubtotal(price.0 * quantity.0)
    })?;
    application.derive(
        |price: &UnitPrice, quantity: &Quantity, shipping: &Shipping| {
            LineTotal(price.0 * quantity.0 + shipping.0)
        },
    )?;

    let row_a = Key::new(12, 1);
    let row_b = Key::new(13, 1);
    application.transact(|tx| {
        tx.spawn(row_a)?;
        tx.spawn(row_b)?;
        tx.put(row_a, UnitPrice(7))?;
        tx.put(row_a, Quantity(3))?;
        tx.put(row_a, Shipping(2))?;
        tx.put(row_b, UnitPrice(5))?;
        tx.put(row_b, Quantity(4))?;
        tx.put(row_b, Shipping(1))?;
        Ok(())
    })?;
    assert_eq!(application.get::<LineSubtotal>(row_a), Some(LineSubtotal(21)));
    assert_eq!(application.get::<LineTotal>(row_a), Some(LineTotal(23)));
    assert_eq!(application.get::<LineTotal>(row_b), Some(LineTotal(21)));

    application.transact(|tx| tx.put(row_a, Quantity(4)))?;
    assert_eq!(application.get::<LineSubtotal>(row_a), Some(LineSubtotal(28)));
    assert_eq!(application.get::<LineTotal>(row_a), Some(LineTotal(30)));
    assert_eq!(application.get::<LineTotal>(row_b), Some(LineTotal(21)));

    application.transact(|tx| tx.remove::<Quantity>(row_a))?;
    assert_eq!(application.get::<LineSubtotal>(row_a), None);
    assert_eq!(application.get::<LineTotal>(row_a), None);
    assert_eq!(
        application.changes::<LineTotal>(),
        vec![OutputChange::Removed(row_a)]
    );

    // The same engine still rejects direct mutation of rule-owned data.
    assert_eq!(
        application.transact(|tx| tx.put(row_b, LineTotal(999))),
        Err(Error::DerivedIsReadOnly)
    );
    assert_eq!(application.get::<LineTotal>(row_b), Some(LineTotal(21)));
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentText(String);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WordCount(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MaxLength(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IsValid(bool);

#[test]
fn same_function_contract_derives_document_and_form_values() -> Result<(), Error> {
    let mut application = Engine::new();
    application.derive(|text: &DocumentText| WordCount(text.0.split_whitespace().count()))?;
    application.derive(|text: &DocumentText, max: &MaxLength| {
        IsValid(text.0.chars().count() <= max.0)
    })?;

    let document = Key::new(3, 1);
    application.transact(|tx| {
        tx.spawn(document)?;
        tx.put(document, DocumentText("hello world".to_owned()))?;
        tx.put(document, MaxLength(11))?;
        Ok(())
    })?;
    assert_eq!(application.get::<WordCount>(document), Some(WordCount(2)));
    assert_eq!(application.get::<IsValid>(document), Some(IsValid(true)));

    application.transact(|tx| tx.put(document, MaxLength(5)))?;
    assert_eq!(application.get::<WordCount>(document), Some(WordCount(2)));
    assert_eq!(application.get::<IsValid>(document), Some(IsValid(false)));
    application.transact(|tx| tx.remove::<DocumentText>(document))?;
    assert_eq!(application.get::<WordCount>(document), None);
    assert_eq!(application.get::<IsValid>(document), None);
    Ok(())
}

#[test]
fn global_output_still_requires_an_artificial_record_and_raw_rule_context() -> Result<(), Error> {
    let mut application = Engine::new();
    application.derive(|price: &UnitPrice, count: &Quantity| {
        LineSubtotal(price.0 * count.0)
    })?;

    // This is the *limitation*, not the recommended API: the engine has
    // no distinct notion of unkeyed/global outputs yet.
    let artificial_summary = Key::new(0, 1);
    application.rule::<InvoicedTotal>(move |ctx, key| {
        if key != artificial_summary {
            return Ok(None);
        }
        let sum = ctx
            .all::<LineSubtotal>()?
            .into_iter()
            .try_fold(0_i64, |acc, (_, line)| {
                acc.checked_add(line.0).ok_or(Error::Overflow)
            })?;
        Ok(Some(InvoicedTotal(sum)))
    })?;

    let row = Key::new(1, 1);
    application.transact(|tx| {
        tx.spawn(artificial_summary)?;
        tx.spawn(row)?;
        tx.put(row, UnitPrice(4))?;
        tx.put(row, Quantity(3))?;
        Ok(())
    })?;
    assert_eq!(
        application.get::<InvoicedTotal>(artificial_summary),
        Some(InvoicedTotal(12))
    );
    application.transact(|tx| tx.put(row, Quantity(5)))?;
    assert_eq!(
        application.get::<InvoicedTotal>(artificial_summary),
        Some(InvoicedTotal(20))
    );
    Ok(())
}

#[test]
fn function_candidate_cannot_bypass_generic_rule_cycles() -> Result<(), Error> {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Recursive(i64);

    let mut application = Engine::new();
    application.derive(|value: &Recursive| Recursive(value.0 + 1))?;
    assert_eq!(
        application.transact(|tx| tx.spawn(Key::new(1, 1))),
        Err(Error::RuleCycle)
    );
    Ok(())
}
