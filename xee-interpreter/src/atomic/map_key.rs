use std::rc::Rc;

use chrono::Offset;
use ibig::IBig;
use num_traits::Float;
use ordered_float::OrderedFloat;

use xee_name::Name;

use super::cast_numeric::exact_decimal;
use super::{
    Atomic, BinaryType, Duration, GDay, GMonth, GMonthDay, GYear, GYearMonth, ToDateTimeStamp,
};

// A map key is constructed according to the rules in
// https://www.w3.org/TR/xpath-functions-31/#func-same-key
// We can use the MapKey as a key in a HashMap so we can implement
// XPath Map

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MapKey {
    String(Rc<String>),
    PositiveInfinity,
    NegativeInfinity,
    NaN,
    Integer(Rc<IBig>),
    Decimal(Rc<ExactDecimal>),
    Duration(Rc<Duration>),
    // datetime with timezone don't hash the same, so we convert
    // into a naive datetime
    Date(chrono::NaiveDateTime),
    NaiveDate(chrono::NaiveDate),
    Time(chrono::NaiveDateTime),
    NaiveTime(chrono::NaiveTime),
    DateTime(chrono::NaiveDateTime),
    NaiveDateTime(chrono::NaiveDateTime),
    GYear(Rc<GYear>),
    GYearMonth(Rc<GYearMonth>),
    GMonth(Rc<GMonth>),
    GMonthDay(Rc<GMonthDay>),
    GDay(Rc<GDay>),
    Boolean(bool),
    Binary(BinaryType, Rc<Vec<u8>>),
    QName(Rc<Name>),
}

#[cfg(target_arch = "x86_64")]
static_assertions::assert_eq_size!(MapKey, [u8; 16]);

/// A number that is not an integer, exactly: `coefficient / 10^scale`, with
/// `scale >= 1` and no trailing zero digit in `coefficient`, so two equal
/// values always have the same fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExactDecimal {
    coefficient: IBig,
    scale: usize,
}

impl MapKey {
    pub(crate) fn new(atomic: Atomic) -> MapKey {
        match &atomic {
            // string types (including AnyURI) and untyped are stored as the same key
            Atomic::String(_, s) | Atomic::Untyped(s) => MapKey::String(s.to_string().into()),
            // op:same-key compares decimals, doubles and floats as exact
            // decimal numbers, so a double is keyed by its exact value,
            // not by a rounded decimal
            Atomic::Float(OrderedFloat(f)) => Self::float(*f),
            Atomic::Double(OrderedFloat(f)) => Self::float(*f),
            Atomic::Decimal(d) => {
                // normalize strips trailing zeros, so 1.50 and 1.5 (and a
                // double of the same value) have the same fields; integral
                // decimals are keyed as integers
                let d = d.normalize();
                let coefficient = IBig::from(d.mantissa());
                if d.scale() == 0 {
                    MapKey::Integer(coefficient.into())
                } else {
                    MapKey::Decimal(
                        ExactDecimal {
                            coefficient,
                            scale: d.scale() as usize,
                        }
                        .into(),
                    )
                }
            }
            Atomic::Integer(_, i) => MapKey::Integer(i.clone()),

            // All types of duration as stored the same way, so they
            // can have the same key
            Atomic::Duration(d) => MapKey::Duration(d.clone()),
            Atomic::YearMonthDuration(d) => {
                MapKey::Duration(Duration::from_year_month(d.clone()).into())
            }
            Atomic::DayTimeDuration(d) => {
                MapKey::Duration(Duration::from_day_time(*d.as_ref()).into())
            }
            // date times with a timezone are stored as a chrono datetime,
            // or they are stored as a naive datetime
            Atomic::DateTime(d) => {
                if d.offset.is_some() {
                    MapKey::DateTime(d.to_naive_date_time(chrono::offset::Utc.fix()))
                } else {
                    MapKey::NaiveDateTime(d.date_time)
                }
            }
            Atomic::DateTimeStamp(d) => MapKey::DateTime(d.naive_local()),
            // times and dates with a timezone are stored as a chrono
            // datetime (but separately), or they are stored as a naive
            // time or date
            Atomic::Time(t) => {
                if t.offset.is_some() {
                    MapKey::Time(t.to_naive_date_time(chrono::offset::Utc.fix()))
                } else {
                    MapKey::NaiveTime(t.time)
                }
            }
            Atomic::Date(d) => {
                if d.offset.is_some() {
                    MapKey::Date(d.to_naive_date_time(chrono::offset::Utc.fix()))
                } else {
                    MapKey::NaiveDate(d.date)
                }
            }
            // gregorian objects have hashes that are already okay
            Atomic::GYearMonth(g) => MapKey::GYearMonth(g.clone()),
            Atomic::GYear(g) => MapKey::GYear(g.clone()),
            Atomic::GMonthDay(g) => MapKey::GMonthDay(g.clone()),
            Atomic::GDay(g) => MapKey::GDay(g.clone()),
            Atomic::GMonth(g) => MapKey::GMonth(g.clone()),
            // booleans are stored as themselves
            Atomic::Boolean(b) => MapKey::Boolean(*b),
            // binary types are stored as themselves
            Atomic::Binary(t, b) => MapKey::Binary(*t, b.to_vec().into()),
            // qnames are stored as themselves
            Atomic::QName(q) => MapKey::QName(q.clone()),
        }
    }

    fn float<F: Float>(f: F) -> MapKey {
        if f.is_nan() {
            return MapKey::NaN;
        }
        if f.is_infinite() {
            return if f.is_sign_positive() {
                MapKey::PositiveInfinity
            } else {
                MapKey::NegativeInfinity
            };
        }
        let (coefficient, scale) = exact_decimal(f);
        if scale == 0 {
            MapKey::Integer(coefficient.into())
        } else {
            MapKey::Decimal(ExactDecimal { coefficient, scale }.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::atomic::NaiveDateTimeWithOffset;

    use super::*;

    use ibig::ibig;
    use rust_decimal_macros::*;

    #[test]
    fn test_float_and_decimal() {
        let a: Atomic = dec!(1.5).into();
        let b: Atomic = (1.5f32).into();
        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_float_and_decimal_that_are_integers() {
        let a: Atomic = dec!(1.0).into();
        let b: Atomic = (1.0f32).into();
        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_float_and_integer() {
        let a: Atomic = dec!(1.0).into();
        let b: Atomic = ibig!(1).into();
        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_decimal_and_integer() {
        let a: Atomic = dec!(1.0).into();
        let b: Atomic = ibig!(1).into();
        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_decimal_trailing_zeros() {
        let a: Atomic = dec!(1.50).into();
        let b: Atomic = dec!(1.5).into();
        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_double_is_keyed_exactly() {
        // 0.1 as a double is 3602879701896397 / 2^55
        let key = MapKey::new(0.1f64.into());
        let coefficient = IBig::from(3602879701896397u64) * IBig::from(5).pow(55);
        assert_eq!(
            key,
            MapKey::Decimal(
                ExactDecimal {
                    coefficient,
                    scale: 55
                }
                .into()
            )
        );
        let decimal: Atomic = dec!(0.1).into();
        assert_ne!(key, MapKey::new(decimal));
        assert_ne!(key, MapKey::new(0.1f32.into()));
    }

    #[test]
    fn test_double_and_decimal_with_the_same_exact_value() {
        // 2^-20 and -2^-20
        for (double, decimal) in [
            (0.00000095367431640625f64, dec!(0.00000095367431640625)),
            (-0.00000095367431640625f64, dec!(-0.00000095367431640625)),
            (-2.75f64, dec!(-2.750)),
        ] {
            let decimal: Atomic = decimal.into();
            assert_eq!(MapKey::new(double.into()), MapKey::new(decimal));
        }
    }

    #[test]
    fn test_integral_doubles_are_integers() {
        assert_eq!(
            MapKey::new(1e30f64.into()),
            MapKey::Integer(
                "1000000000000000019884624838656"
                    .parse::<IBig>()
                    .unwrap()
                    .into()
            )
        );
        assert_eq!(
            MapKey::new((-6.0f32).into()),
            MapKey::Integer(IBig::from(-6).into())
        );
        assert_eq!(
            MapKey::new((-0.0f64).into()),
            MapKey::Integer(IBig::from(0).into())
        );
    }

    #[test]
    fn test_integer_and_bool() {
        let a: Atomic = ibig!(1).into();
        let b: Atomic = true.into();
        assert_ne!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_string_and_untyped() {
        let a: Atomic = "foo".into();
        let b: Atomic = Atomic::Untyped("foo".into());
        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_datetimes_with_timezones() {
        let a_date_time = NaiveDateTimeWithOffset::new(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 2)
                .unwrap()
                .and_hms_milli_opt(1, 2, 3, 456)
                .unwrap(),
            Some(chrono::offset::Utc.fix()),
        );
        // put it at the same time, but in timezone one hour ahead
        let b_date_time = NaiveDateTimeWithOffset::new(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 2)
                .unwrap()
                .and_hms_milli_opt(2, 2, 3, 456)
                .unwrap(),
            Some(chrono::FixedOffset::east_opt(60 * 60).unwrap()),
        );

        let a: Atomic = Atomic::DateTime(a_date_time.into());
        let b: Atomic = Atomic::DateTime(b_date_time.into());

        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_datetimes_without_timezones() {
        let a_date_time = NaiveDateTimeWithOffset::new(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 2)
                .unwrap()
                .and_hms_milli_opt(3, 2, 3, 456)
                .unwrap(),
            None,
        );
        let b_date_time = NaiveDateTimeWithOffset::new(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 2)
                .unwrap()
                .and_hms_milli_opt(3, 2, 3, 456)
                .unwrap(),
            None,
        );

        let a: Atomic = Atomic::DateTime(a_date_time.into());
        let b: Atomic = Atomic::DateTime(b_date_time.into());

        assert_eq!(MapKey::new(a), MapKey::new(b));
    }

    #[test]
    fn test_datetimes_with_and_without_timezones() {
        let a_date_time = NaiveDateTimeWithOffset::new(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 2)
                .unwrap()
                .and_hms_milli_opt(3, 2, 3, 456)
                .unwrap(),
            Some(chrono::offset::Utc.fix()),
        );
        let b_date_time = NaiveDateTimeWithOffset::new(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 2)
                .unwrap()
                .and_hms_milli_opt(3, 2, 3, 456)
                .unwrap(),
            None,
        );

        let a: Atomic = Atomic::DateTime(a_date_time.into());
        let b: Atomic = Atomic::DateTime(b_date_time.into());

        assert_ne!(MapKey::new(a), MapKey::new(b));
    }
}
