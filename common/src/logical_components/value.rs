use crate::{logical_components::Expression, schema::instruction::SerializationType};
use borsh::{BorshDeserialize, BorshSerialize};
use borsh_boxed::{BorshDeserializeBoxed, BorshSerializeBoxed};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValueType {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
}

#[derive(Clone, Debug, Eq, BorshDeserializeBoxed, BorshSerializeBoxed)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Value {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    I128(i128),
    U128(u128),
    Bytes(Vec<u8>),
    Vec(Vec<Value>),
    Option(Option<Box<Value>>),
    Struct(Vec<(String, Value)>),
    String(String),
    Bool(bool),
}

impl Value {
    pub fn expr(self) -> Expression {
        Expression::Literal(self)
    }

    pub fn as_u128(&self) -> u128 {
        match self {
            Value::U8(v) => *v as u128,
            Value::U16(v) => *v as u128,
            Value::U32(v) => *v as u128,
            Value::U64(v) => *v as u128,
            Value::I8(v) => *v as u128,
            Value::I16(v) => *v as u128,
            Value::I32(v) => *v as u128,
            Value::I64(v) => *v as u128,
            Value::I128(v) => *v as u128,
            Value::U128(v) => *v,
            Value::Bytes(_) => panic!("Cannot convert bytes to u128"),
            Value::Option(v) => {
                if let Some(v) = v {
                    v.as_u128()
                } else {
                    panic!("Cannot convert None to u128")
                }
            }
            Value::Struct(_) => panic!("Cannot convert struct to u128"),
            Value::String(_) => panic!("Cannot convert string to u128"),
            Value::Bool(_) => panic!("Cannot convert bool to u128"),
            Value::Vec(_) => panic!("Cannot convert vec to u128"),
        }
    }

    pub fn as_i128(&self) -> i128 {
        match self {
            Value::U8(v) => *v as i128,
            Value::I8(v) => *v as i128,
            Value::U16(v) => *v as i128,
            Value::I16(v) => *v as i128,
            Value::U32(v) => *v as i128,
            Value::I32(v) => *v as i128,
            Value::U64(v) => *v as i128,
            Value::I64(v) => *v as i128,
            Value::I128(v) => *v,
            Value::U128(v) => *v as i128,
            Value::Bytes(_) => panic!("Cannot convert bytes to i128"),
            Value::Option(v) => {
                if let Some(v) = v {
                    v.as_i128()
                } else {
                    panic!("Cannot convert None to i128")
                }
            }
            Value::Struct(_) => panic!("Cannot convert struct to i128"),
            Value::String(_) => panic!("Cannot convert string to i128"),
            Value::Bool(_) => panic!("Cannot convert bool to i128"),
            Value::Vec(_) => panic!("Cannot convert vec to i128"),
        }
    }

    pub fn as_bytes(&self, serialization_type: SerializationType) -> Vec<u8> {
        match self {
            Value::U8(v) => v.to_le_bytes().to_vec(),
            Value::I8(v) => v.to_le_bytes().to_vec(),
            Value::U16(v) => v.to_le_bytes().to_vec(),
            Value::I16(v) => v.to_le_bytes().to_vec(),
            Value::U32(v) => v.to_le_bytes().to_vec(),
            Value::I32(v) => v.to_le_bytes().to_vec(),
            Value::U64(v) => v.to_le_bytes().to_vec(),
            Value::I64(v) => v.to_le_bytes().to_vec(),
            Value::I128(v) => v.to_le_bytes().to_vec(),
            Value::U128(v) => v.to_le_bytes().to_vec(),
            Value::Bytes(v) => v.clone(),
            Value::Vec(v) => {
                // TODO: dog shit
                let mut bytes = vec![0, 0, 0, v.len() as u8];
                for value in v.iter() {
                    bytes.extend_from_slice(&value.as_bytes(serialization_type));
                }
                bytes
            }
            Value::Option(v) => match serialization_type {
                SerializationType::Borsh => {
                    let mut bytes = vec![];
                    if let Some(v) = v {
                        bytes.extend_from_slice(&[1]);
                        bytes.extend_from_slice(&v.as_bytes(serialization_type));
                    } else {
                        bytes.extend_from_slice(&[0]);
                    }
                    bytes
                }
                SerializationType::C => {
                    panic!("C serialization not supported for structs");
                }
                SerializationType::Bytemuck => {
                    panic!("Bytes serialization not supported for structs");
                }
            },
            Value::Struct(fields) => match serialization_type {
                SerializationType::Borsh => {
                    let mut bytes = vec![];
                    for (name, value) in fields.iter() {
                        bytes.extend_from_slice(&value.as_bytes(serialization_type));
                    }
                    bytes
                }
                SerializationType::C => {
                    panic!("C serialization not supported for structs");
                }
                SerializationType::Bytemuck => {
                    panic!("Bytes serialization not supported for structs");
                }
            },
            Value::String(v) => v.try_to_vec().unwrap(),
            Value::Bool(v) => vec![*v as u8],
        }
    }

    pub fn safe_cast(&self, target: ValueType) -> Result<Value, String> {
        let v = if self.is_unsigned() {
            self.as_u128() as i128
        } else {
            self.as_i128()
        };
        match target {
            ValueType::U8 => {
                if (0..=u8::MAX as i128).contains(&v) {
                    Ok(Value::U8(v as u8))
                } else {
                    Err(format!("Out of range for U8: {}", v))
                }
            }
            ValueType::I8 => {
                if (i8::MIN as i128..=i8::MAX as i128).contains(&v) {
                    Ok(Value::I8(v as i8))
                } else {
                    Err("Out of range for I8".into())
                }
            }
            ValueType::U16 => {
                if (0..=u16::MAX as i128).contains(&v) {
                    Ok(Value::U16(v as u16))
                } else {
                    Err("Out of range for U16".into())
                }
            }
            ValueType::I16 => {
                if (i16::MIN as i128..=i16::MAX as i128).contains(&v) {
                    Ok(Value::I16(v as i16))
                } else {
                    Err("Out of range for I16".into())
                }
            }
            ValueType::U32 => {
                if (0..=u32::MAX as i128).contains(&v) {
                    Ok(Value::U32(v as u32))
                } else {
                    Err("Out of range for U32".into())
                }
            }
            ValueType::I32 => {
                if (i32::MIN as i128..=i32::MAX as i128).contains(&v) {
                    Ok(Value::I32(v as i32))
                } else {
                    Err("Out of range for I32".into())
                }
            }
            ValueType::U64 => {
                if (0..=u64::MAX as i128).contains(&v) {
                    Ok(Value::U64(v as u64))
                } else {
                    Err("Out of range for U64".into())
                }
            }
            ValueType::I64 => {
                if (i64::MIN as i128..=i64::MAX as i128).contains(&v) {
                    Ok(Value::I64(v as i64))
                } else {
                    Err("Out of range for I64".into())
                }
            }
        }
    }

    pub fn is_unsigned(&self) -> bool {
        matches!(
            self,
            Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_)
        )
    }

    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_)
        )
    }

    pub fn is_integer(&self) -> bool {
        self.is_unsigned() || self.is_signed()
    }

    // pub fn borsh_serialize(&self) -> Result<Vec<u8>, std::io::Error> {
    //     match self {
    //         Value::U8(v) => v.try_to_vec(),
    //         Value::I8(v) => v.try_to_vec(),
    //         Value::U16(v) => v.try_to_vec(),
    //         Value::I16(v) => v.try_to_vec(),
    //         Value::U32(v) => v.try_to_vec(),
    //         Value::I32(v) => v.try_to_vec(),
    //         Value::U64(v) => v.try_to_vec(),
    //         Value::I64(v) => v.try_to_vec(),
    //         Value::Bytes(v) => Ok(v.clone()),
    //         Value::Option(v) => match v {
    //             Some(v) => v.borsh_serialize(),
    //             None => Ok(vec![]),
    //         },
    //     }
    // }
}

impl PartialEq for Value {
    fn eq(&self, right: &Self) -> bool {
        if self.is_integer() && right.is_integer() {
            // match is signed , unsigned
            let left_signed = self.is_signed();
            let right_signed = right.is_signed();

            if left_signed && right_signed {
                self.as_i128() == right.as_i128()
            } else if left_signed && !right_signed {
                // FOLLOWUP: this could result in cast overflow if number is u128 as being casted to i128
                self.as_i128() == right.as_u128() as i128
            } else if !left_signed && right_signed {
                // FOLLOWUP: this could result in cast overflow if number is u128 as being casted to i128
                self.as_u128() as i128 == right.as_i128()
            } else {
                self.as_u128() == right.as_u128()
            }
        } else {
            match (self, right) {
                (Value::Bytes(a), Value::Bytes(b)) => a.eq(b),
                (Value::Option(a), Value::Option(b)) => a.eq(b),
                _ => false,
            }
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::U8(a), Value::U8(b)) => a.partial_cmp(b),
            (Value::I8(a), Value::I8(b)) => a.partial_cmp(b),
            (Value::U16(a), Value::U16(b)) => a.partial_cmp(b),
            (Value::I16(a), Value::I16(b)) => a.partial_cmp(b),
            (Value::U32(a), Value::U32(b)) => a.partial_cmp(b),
            (Value::I32(a), Value::I32(b)) => a.partial_cmp(b),
            (Value::U64(a), Value::U64(b)) => a.partial_cmp(b),
            (Value::I64(a), Value::I64(b)) => a.partial_cmp(b),
            (Value::Bytes(a), Value::Bytes(b)) => a.partial_cmp(b),
            (Value::Option(a), Value::Option(b)) => a.partial_cmp(b),
            _ => {
                if self.is_integer() && other.is_integer() {
                    // FOLLOWUP: this should be fine even if the values are signed
                    self.as_u128().partial_cmp(&other.as_u128())
                } else {
                    None
                }
            }
        }
    }
}

// Macro for checked operations (returning Option)
macro_rules! impl_checked_arithmetic {
    ($op:ident, $fn_name:ident, $trait_fn:ident, $err_msg:expr) => {
        impl Value {
            pub fn $fn_name(&self, rhs: &Self) -> Result<Value, String> {
                macro_rules! $op {
                    ($a:expr, $b:expr) => {
                        $a.$trait_fn($b)
                    };
                }

                macro_rules! format_err {
                    ($a:expr, $b:expr) => {
                        format!("{} (left: {:?}, right: {:?})", $err_msg, $a, $b)
                    };
                }

                match (self, rhs) {
                    (Value::U8(a), Value::U8(b)) => {
                        $op!(*a, *b).map(Value::U8).ok_or_else(|| format_err!(a, b))
                    }
                    (Value::I8(a), Value::I8(b)) => {
                        $op!(*a, *b).map(Value::I8).ok_or_else(|| format_err!(a, b))
                    }
                    (Value::U16(a), Value::U16(b)) => $op!(*a, *b)
                        .map(Value::U16)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::I16(a), Value::I16(b)) => $op!(*a, *b)
                        .map(Value::I16)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::U32(a), Value::U32(b)) => $op!(*a, *b)
                        .map(Value::U32)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::I32(a), Value::I32(b)) => $op!(*a, *b)
                        .map(Value::I32)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::U64(a), Value::U64(b)) => $op!(*a, *b)
                        .map(Value::U64)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::I64(a), Value::I64(b)) => $op!(*a, *b)
                        .map(Value::I64)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::U128(a), Value::U128(b)) => $op!(*a, *b)
                        .map(Value::U128)
                        .ok_or_else(|| format_err!(a, b)),
                    (Value::I128(a), Value::I128(b)) => $op!(*a, *b)
                        .map(Value::I128)
                        .ok_or_else(|| format_err!(a, b)),
                    _ => {
                        let (a, b) = match (self, rhs) {
                            (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U128(b)) => (self.as_i128(), (*b as u128) as i128),
                            _ => (self.as_i128(), rhs.as_i128()),
                        };
                        $op!(a, b).map(Value::I128).ok_or_else(|| format_err!(a, b))
                    }
                }
            }
        }
    };
}

// Macro for saturating and wrapping operations (returning the value directly)
macro_rules! impl_unchecked_arithmetic {
    ($op:ident, $fn_name:ident, $trait_fn:ident) => {
        impl Value {
            pub fn $fn_name(&self, rhs: &Self) -> Result<Value, String> {
                macro_rules! $op {
                    ($a:expr, $b:expr) => {
                        $a.$trait_fn($b)
                    };
                }

                Ok(match (self, rhs) {
                    (Value::U8(a), Value::U8(b)) => Value::U8($op!(*a, *b)),
                    (Value::I8(a), Value::I8(b)) => Value::I8($op!(*a, *b)),
                    (Value::U16(a), Value::U16(b)) => Value::U16($op!(*a, *b)),
                    (Value::I16(a), Value::I16(b)) => Value::I16($op!(*a, *b)),
                    (Value::U32(a), Value::U32(b)) => Value::U32($op!(*a, *b)),
                    (Value::I32(a), Value::I32(b)) => Value::I32($op!(*a, *b)),
                    (Value::U64(a), Value::U64(b)) => Value::U64($op!(*a, *b)),
                    (Value::I64(a), Value::I64(b)) => Value::I64($op!(*a, *b)),
                    (Value::U128(a), Value::U128(b)) => Value::U128($op!(*a, *b)),
                    (Value::I128(a), Value::I128(b)) => Value::I128($op!(*a, *b)),
                    _ => {
                        let (a, b) = match (self, rhs) {
                            (Value::U8(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U16(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U32(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U64(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (Value::U128(a), _) => ((*a as u128) as i128, rhs.as_i128()),
                            (_, Value::U8(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U16(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U32(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U64(b)) => (self.as_i128(), (*b as u128) as i128),
                            (_, Value::U128(b)) => (self.as_i128(), (*b as u128) as i128),
                            _ => (self.as_i128(), rhs.as_i128()),
                        };
                        Value::I128($op!(a, b))
                    }
                })
            }
        }
    };
}

// Implement checked multiplication
impl_checked_arithmetic!(
    checked_op,
    checked_mul,
    checked_mul,
    "Overflow in checked multiplication"
);

impl_checked_arithmetic!(
    checked_op,
    checked_sub,
    checked_sub,
    "Overflow in checked subtraction"
);

impl_checked_arithmetic!(
    checked_op,
    checked_add,
    checked_add,
    "Overflow in checked addition"
);

impl_checked_arithmetic!(
    checked_op,
    checked_div,
    checked_div,
    "Division by zero or overflow in checked division"
);

// Implement saturating multiplication
impl_unchecked_arithmetic!(saturating_op, saturating_mul, saturating_mul);

// Implement saturating addition
impl_unchecked_arithmetic!(saturating_op, saturating_add, saturating_add);

// Implement saturating subtraction
impl_unchecked_arithmetic!(saturating_op, saturating_sub, saturating_sub);

// Implement saturating division
impl_unchecked_arithmetic!(saturating_op, saturating_div, saturating_div);

// Implement wrapped multiplication
impl_unchecked_arithmetic!(wrapping_op, wrapping_mul, wrapping_mul);

// Implement wrapped addition
impl_unchecked_arithmetic!(wrapping_op, wrapping_add, wrapping_add);

// Implement wrapped subtraction
impl_unchecked_arithmetic!(wrapping_op, wrapping_sub, wrapping_sub);

// Implement wrapped division
impl_unchecked_arithmetic!(wrapping_op, wrapping_div, wrapping_div);

// impl<T> TryFrom<T> for Value
// where
//     T: TryInto<Value>,
//     <T as TryInto<Value>>::Error: std::fmt::Debug,
// {
//     type Error = String;

//     fn try_from(value: T) -> Result<Self, Self::Error> {
//         value
//             .try_into()
//             .map_err(|e| format!("Conversion error: {:?}", e))
//     }
// }

// macro_rules! impl_try_into_value {
//     ($($t:ty => $variant:ident),*) => {
//         $(
//             impl TryInto<Value> for $t {
//                 type Error = String;

//                 fn try_into(self) -> Result<Value, Self::Error> {
//                     Ok(Value::$variant(self))
//                 }
//             }
//         )*
//     };
// }

// impl_try_into_value! {
//     u8 => U8,
//     i8 => I8,
//     u16 => U16,
//     i16 => I16,
//     u32 => U32,
//     i32 => I32,
//     u64 => U64,
//     i64 => I64,
//     i128 => I128,
//     u128 => U128
// }

// pub struct MetadataArgs {
//     /// The name of the asset
//     pub name: String,
//     /// The symbol for the asset
//     pub symbol: String,
//     /// URI pointing to JSON representing the asset
//     pub uri: String,
//     /// Royalty basis points that goes to creators in secondary sales (0-10000)
//     pub seller_fee_basis_points: u16,
//     pub primary_sale_happened: bool,
//     pub is_mutable: bool,
//     /// nonce for easy calculation of editions, if present
//     pub edition_nonce: Option<u8>,
//     /// Since we cannot easily change Metadata, we add the new DataV2 fields here at the end.
//     pub token_standard: Option<TokenStandard>,
//     /// Collection
//     pub collection: Option<Collection>,
//     /// Uses
//     pub uses: Option<Uses>,
//     pub token_program_version: TokenProgramVersion,
//     pub creators: Vec<Creator>,
// }
