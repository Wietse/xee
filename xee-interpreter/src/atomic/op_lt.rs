use std::cmp::Ordering;

use ordered_float::OrderedFloat;

use crate::error;

use super::cast_binary::cast_binary_compare;
use super::datetime::OrdWithDefaultOffset;
use super::{Atomic, AtomicCompare, AtomicCompareValue, BinaryType};

pub(crate) struct OpLt;

impl AtomicCompare for OpLt {
    fn atomic_compare<F>(
        a: Atomic,
        b: Atomic,
        string_compare: F,
        default_offset: chrono::FixedOffset,
    ) -> error::Result<bool>
    where
        F: Fn(&str, &str) -> Ordering,
    {
        let (a, b) = cast_binary_compare(a, b)?;

        use Atomic::*;

        match (a, b) {
            (Decimal(a), Decimal(b)) => Ok(a < b),
            (Integer(_, a), Integer(_, b)) => Ok(a < b),
            // Compare the raw floats, not the OrderedFloat wrappers: IEEE-754
            // requires every ordering comparison involving NaN to be false,
            // whereas OrderedFloat imposes a total order that sorts NaN above
            // all other values.
            (Float(OrderedFloat(a)), Float(OrderedFloat(b))) => Ok(a < b),
            (Double(OrderedFloat(a)), Double(OrderedFloat(b))) => Ok(a < b),
            (Boolean(a), Boolean(b)) => Ok(!a & b),
            (String(_, a), String(_, b)) => Ok(string_compare(a.as_ref(), b.as_ref()).is_lt()),
            (Date(a), Date(b)) => Ok(a
                .as_ref()
                .cmp_with_default_offset(b.as_ref(), default_offset)
                .is_lt()),
            (Time(a), Time(b)) => Ok(a
                .as_ref()
                .cmp_with_default_offset(b.as_ref(), default_offset)
                .is_lt()),
            (DateTime(a), DateTime(b)) => Ok(a
                .as_ref()
                .cmp_with_default_offset(b.as_ref(), default_offset)
                .is_lt()),
            (DateTimeStamp(a), DateTimeStamp(b)) => Ok(a < b),
            (YearMonthDuration(a), YearMonthDuration(b)) => Ok(a < b),
            (DayTimeDuration(a), DayTimeDuration(b)) => Ok(a < b),
            (Binary(BinaryType::Hex, a), Binary(BinaryType::Hex, b)) => Ok(a < b),
            (Binary(BinaryType::Base64, a), Binary(BinaryType::Base64, b)) => Ok(a < b),
            _ => Err(error::Error::XPTY0004),
        }
    }

    fn arguments_inverted() -> impl AtomicCompare {
        super::OpGt
    }

    fn value() -> AtomicCompareValue {
        AtomicCompareValue::Lt
    }
}

#[cfg(test)]
mod tests {
    use chrono::Offset;
    use ordered_float::OrderedFloat;

    use super::OpLt;
    use crate::atomic::{Atomic, AtomicCompare, OpGe, OpGt, OpLe};

    fn default_offset() -> chrono::FixedOffset {
        chrono::offset::Utc.fix()
    }

    fn assert_all_ops_false(a: Atomic, b: Atomic) {
        for result in [
            OpLt::atomic_compare(a.clone(), b.clone(), str::cmp, default_offset()),
            OpLe::atomic_compare(a.clone(), b.clone(), str::cmp, default_offset()),
            OpGt::atomic_compare(a.clone(), b.clone(), str::cmp, default_offset()),
            OpGe::atomic_compare(a.clone(), b.clone(), str::cmp, default_offset()),
        ] {
            assert!(!result.unwrap());
        }
    }

    // IEEE-754: every ordering comparison involving NaN is false. This must
    // hold for both xs:double and xs:float, and regardless of which operand
    // is NaN.
    #[test]
    fn test_double_nan_ordering_is_always_false() {
        let nan = Atomic::Double(OrderedFloat(f64::NAN));
        let two = Atomic::Double(OrderedFloat(2.0));
        assert_all_ops_false(nan.clone(), two.clone());
        assert_all_ops_false(two, nan.clone());
        assert_all_ops_false(nan.clone(), nan);
    }

    #[test]
    fn test_float_nan_ordering_is_always_false() {
        let nan = Atomic::Float(OrderedFloat(f32::NAN));
        let two = Atomic::Float(OrderedFloat(2.0));
        assert_all_ops_false(nan.clone(), two.clone());
        assert_all_ops_false(two, nan.clone());
        assert_all_ops_false(nan.clone(), nan);
    }
}
