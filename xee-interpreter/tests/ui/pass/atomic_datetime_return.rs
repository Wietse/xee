//! `xs:date`, `xs:time`, `xs:dateTime` return values constructed via the
//! public `try_new` constructors. Compiles only when
//! `NaiveDateWithOffset::try_new`, `NaiveTimeWithOffset::try_new`, and
//! `NaiveDateTimeWithOffset::try_new` are publicly reachable from outside
//! `xee-interpreter`.

use xee_interpreter::atomic::{NaiveDateTimeWithOffset, NaiveDateWithOffset, NaiveTimeWithOffset};
use xee_interpreter::error;
use xee_xpath_macros::xpath_fn;

#[xpath_fn("fn:my_date() as xs:date")]
fn my_date() -> error::Result<NaiveDateWithOffset> {
    NaiveDateWithOffset::try_new(chrono::NaiveDate::from_ymd_opt(2026, 5, 20).unwrap(), None)
}

#[xpath_fn("fn:my_time() as xs:time")]
fn my_time() -> error::Result<NaiveTimeWithOffset> {
    NaiveTimeWithOffset::try_new(chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap(), None)
}

#[xpath_fn("fn:my_date_time() as xs:dateTime")]
fn my_date_time() -> error::Result<NaiveDateTimeWithOffset> {
    NaiveDateTimeWithOffset::try_new(
        chrono::NaiveDate::from_ymd_opt(2026, 5, 20)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap(),
        None,
    )
}

fn main() {
    let _ = (my_date, my_time, my_date_time);
}
