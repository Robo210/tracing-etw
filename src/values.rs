use std::borrow::Cow;
use std::fmt::Write;

use tracing::field;

use crate::native::EventBuilderWrapper;

#[allow(non_camel_case_types)]
pub(crate) enum ValueTypes {
    None,
    v_u64(u64),
    v_i64(i64),
    v_u128(u128),
    v_i128(i128),
    v_f64(f64),
    v_bool(bool),
    v_str(Cow<'static, str>), // Would be nice if we didn't have to do a heap allocation
    v_char(char),
}

impl Default for ValueTypes {
    fn default() -> Self {
        ValueTypes::None
    }
}

impl From<u64> for ValueTypes {
    fn from(value: u64) -> Self {
        ValueTypes::v_u64(value)
    }
}

impl From<i64> for ValueTypes {
    fn from(value: i64) -> Self {
        ValueTypes::v_i64(value)
    }
}

impl From<u128> for ValueTypes {
    fn from(value: u128) -> Self {
        ValueTypes::v_u128(value)
    }
}

impl From<i128> for ValueTypes {
    fn from(value: i128) -> Self {
        ValueTypes::v_i128(value)
    }
}

impl From<f64> for ValueTypes {
    fn from(value: f64) -> Self {
        ValueTypes::v_f64(value)
    }
}

impl From<bool> for ValueTypes {
    fn from(value: bool) -> Self {
        ValueTypes::v_bool(value)
    }
}

impl From<&'static str> for ValueTypes {
    fn from(value: &'static str) -> Self {
        ValueTypes::v_str(Cow::from(value))
    }
}

impl From<String> for ValueTypes {
    fn from(value: String) -> Self {
        ValueTypes::v_str(Cow::from(value))
    }
}

impl From<char> for ValueTypes {
    fn from(value: char) -> Self {
        ValueTypes::v_char(value)
    }
}

pub(crate) struct FieldAndValue<'a> {
    #[allow(dead_code)]
    pub(crate) field_name: &'static str,
    #[allow(dead_code)]
    pub(crate) value: &'a ValueTypes,
}

pub(crate) struct ValueVisitor<'a> {
    pub(crate) fields: &'a [&'static str],
    pub(crate) values: &'a mut [ValueTypes],
    pub(crate) indexes: &'a mut [u8],
}

impl<'a> ValueVisitor<'a> {
    fn update_value(&mut self, field_name: &'static str, value: ValueTypes) {
        let res = self
            .indexes
            .binary_search_by_key(&field_name, |idx| self.fields[*idx as usize]);
        if let Err(_) = res {
            return; // We don't support (and don't need to support) adding new fields that weren't in the original metadata
        } else {
            self.values[self.indexes[res.unwrap()] as usize] = value;
        }
    }
}

impl<'a> field::Visit for ValueVisitor<'a> {
    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        let mut string = String::with_capacity(10); // Just a guess
        if write!(string, "{:?}", value).is_err() {
            return;
        }

        self.update_value(field.name(), ValueTypes::v_str(Cow::from(string)));
    }

    fn record_f64(&mut self, field: &field::Field, value: f64) {
        self.update_value(field.name(), ValueTypes::v_f64(value));
    }

    fn record_i64(&mut self, field: &field::Field, value: i64) {
        self.update_value(field.name(), ValueTypes::v_i64(value));
    }

    fn record_u64(&mut self, field: &field::Field, value: u64) {
        self.update_value(field.name(), ValueTypes::v_u64(value));
    }

    fn record_i128(&mut self, field: &field::Field, value: i128) {
        self.update_value(field.name(), ValueTypes::v_i128(value));
    }

    fn record_u128(&mut self, field: &field::Field, value: u128) {
        self.update_value(field.name(), ValueTypes::v_u128(value));
    }

    fn record_bool(&mut self, field: &field::Field, value: bool) {
        self.update_value(field.name(), ValueTypes::v_bool(value));
    }

    fn record_str(&mut self, field: &field::Field, value: &str) {
        self.update_value(
            field.name(),
            ValueTypes::v_str(Cow::from(value.to_string())),
        );
    }

    fn record_error(&mut self, _field: &field::Field, _value: &(dyn std::error::Error + 'static)) {}
}

pub(crate) trait AddFieldAndValue {
    fn add_field_value(&mut self, fv: &crate::values::FieldAndValue);
}

impl<'a> field::Visit for EventBuilderWrapper<'a> {
    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        let mut string = String::with_capacity(10);
        if write!(string, "{:?}", value).is_err() {
            // TODO: Needs to do a heap allocation
            return;
        }

        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(string),
        })
    }

    fn record_f64(&mut self, field: &field::Field, value: f64) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value),
        })
    }

    fn record_i64(&mut self, field: &field::Field, value: i64) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value),
        })
    }

    fn record_u64(&mut self, field: &field::Field, value: u64) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value),
        })
    }

    fn record_i128(&mut self, field: &field::Field, value: i128) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value),
        })
    }

    fn record_u128(&mut self, field: &field::Field, value: u128) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value),
        })
    }

    fn record_bool(&mut self, field: &field::Field, value: bool) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value),
        })
    }

    fn record_str(&mut self, field: &field::Field, value: &str) {
        self.add_field_value(&FieldAndValue {
            field_name: field.name(),
            value: &ValueTypes::from(value.to_string()),
        })
    }

    fn record_error(&mut self, _field: &field::Field, _value: &(dyn std::error::Error + 'static)) {}
}
