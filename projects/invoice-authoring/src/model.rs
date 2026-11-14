//! Independent, explicitly specified invoice-domain expectations in fixtures/cases.json.
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Invoice {
    pub due: u64,
    pub paid: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Line {
    pub invoice_id: u32,
    pub price: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub invoices: BTreeMap<u32, Invoice>,
    pub lines: BTreeMap<u32, Line>,
    pub day: u64,
}

pub fn initial() -> Source {
    Source {
        invoices: BTreeMap::from([
            (
                1,
                Invoice {
                    due: 11,
                    paid: false,
                },
            ),
            (
                2,
                Invoice {
                    due: 14,
                    paid: false,
                },
            ),
        ]),
        lines: BTreeMap::from([
            (
                11,
                Line {
                    invoice_id: 1,
                    price: 10,
                    quantity: 2,
                },
            ),
            (
                12,
                Line {
                    invoice_id: 1,
                    price: 4,
                    quantity: 3,
                },
            ),
            (
                13,
                Line {
                    invoice_id: 2,
                    price: 6,
                    quantity: 5,
                },
            ),
        ]),
        day: 10,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    MissingLine,
    MissingInvoice,
    Overflow,
    DayRegression,
}

impl Rejection {
    pub fn id(&self) -> &'static str {
        match self {
            Self::MissingLine => "missing_line",
            Self::MissingInvoice => "missing_invoice",
            Self::Overflow => "overflow",
            Self::DayRegression => "day_regression",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Edit {
    Quantity { line: u32, value: i64 },
    Price { line: u32, value: i64 },
    Reassign { line: u32, invoice: u32 },
    Day { value: u64 },
    Paid { invoice: u32, value: bool },
    Delete { line: u32 },
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Projection {
    pub a_total: i64,
    pub b_total: i64,
    pub open_balance: i64,
    pub a_overdue: bool,
    pub b_overdue: bool,
}

impl Projection {
    pub fn changes(self, next: Self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.a_total != next.a_total {
            fields.push("a_total");
        }
        if self.b_total != next.b_total {
            fields.push("b_total");
        }
        if self.open_balance != next.open_balance {
            fields.push("open_balance");
        }
        if self.a_overdue != next.a_overdue {
            fields.push("a_overdue");
        }
        if self.b_overdue != next.b_overdue {
            fields.push("b_overdue");
        }
        fields
    }
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub name: String,
    pub edit: Edit,
    pub accepted: bool,
    pub error: Option<String>,
    pub expected: Projection,
    pub changes: Vec<String>,
}

// The domain projection is deliberately *ordinary* Rust, not an incremental
// evaluator or a second source-of-truth. Every call re-evaluates all lines.
pub fn project(source: &Source) -> Result<Projection, Rejection> {
    let mut totals = BTreeMap::<u32, i64>::new();
    for &invoice_id in source.invoices.keys() {
        totals.insert(invoice_id, 0);
    }
    for line in source.lines.values() {
        let total = totals
            .get_mut(&line.invoice_id)
            .ok_or(Rejection::MissingInvoice)?;
        let amount = line
            .price
            .checked_mul(line.quantity)
            .ok_or(Rejection::Overflow)?;
        *total = total.checked_add(amount).ok_or(Rejection::Overflow)?;
    }
    let mut open_balance = 0_i64;
    for (&invoice_id, invoice) in &source.invoices {
        if !invoice.paid {
            open_balance = open_balance
                .checked_add(totals[&invoice_id])
                .ok_or(Rejection::Overflow)?;
        }
    }
    let a = source.invoices.get(&1).ok_or(Rejection::MissingInvoice)?;
    let b = source.invoices.get(&2).ok_or(Rejection::MissingInvoice)?;
    Ok(Projection {
        a_total: totals[&1],
        b_total: totals[&2],
        open_balance,
        a_overdue: !a.paid && source.day > a.due,
        b_overdue: !b.paid && source.day > b.due,
    })
}

// All admitted operations are explicit and closed. The temporary source
// snapshot is an application-level preflight control, NOT a native ECS
// arbitrary-World transaction. Invalid edits never enter either ECS backend.
pub fn preflight(previous: &Source, edit: &Edit) -> Result<Source, Rejection> {
    let mut next = previous.clone();
    match *edit {
        Edit::Quantity { line, value } => {
            next.lines
                .get_mut(&line)
                .ok_or(Rejection::MissingLine)?
                .quantity = value;
        }
        Edit::Price { line, value } => {
            next.lines
                .get_mut(&line)
                .ok_or(Rejection::MissingLine)?
                .price = value;
        }
        Edit::Reassign { line, invoice } => {
            next.lines
                .get_mut(&line)
                .ok_or(Rejection::MissingLine)?
                .invoice_id = invoice;
        }
        Edit::Day { value } => {
            if value < previous.day {
                return Err(Rejection::DayRegression);
            }
            next.day = value;
        }
        Edit::Paid { invoice, value } => {
            next.invoices
                .get_mut(&invoice)
                .ok_or(Rejection::MissingInvoice)?
                .paid = value;
        }
        Edit::Delete { line } => {
            if next.lines.remove(&line).is_none() {
                return Err(Rejection::MissingLine);
            }
        }
        Edit::Noop => {}
    }
    project(&next)?;
    Ok(next)
}
