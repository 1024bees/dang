use anyhow::Context;
use num_bigint::BigUint;
use wellen::SignalValue;

impl Mappable for BigUint {
    fn try_from_signal(signal_value: SignalValue<'_>) -> Option<Self> {
        match signal_value {
            SignalValue::Binary(val, _bits) => Some(BigUint::from_bytes_be(val)),
            _ => None,
        }
    }
}

/// Trait to easily convert between existing data types
pub trait Mappable: Sized + PartialEq {
    fn try_from_signal(signal_value: SignalValue<'_>) -> Option<Self>;
    fn from_signal(signal_value: SignalValue<'_>) -> Self {
        Self::try_from_signal(signal_value)
            .with_context(|| {
                format!(
                    "Failed to convert signal value to {:?}",
                    signal_value.to_bit_string()
                )
            })
            .expect("Failed to convert signal value to Mappable")
    }

    fn bit_width(&self) -> u32 {
        (std::mem::size_of::<Self>() * 8) as u32
    }
}

macro_rules! impl_mappable_basic {
    ($t:ty) => {
        impl Mappable for $t {
            fn try_from_signal(signal_value: SignalValue<'_>) -> Option<Self> {
                match signal_value {
                    SignalValue::Binary(val, bits) => {
                        if bits <= std::mem::size_of::<Self>() as u32 * 8 {
                            let val = val.try_into().ok().map(|val| <$t>::from_be_bytes(val));
                            val
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        }
    };
}

impl_mappable_basic!(u8);
impl_mappable_basic!(u16);
impl_mappable_basic!(u32);
impl_mappable_basic!(u64);
impl_mappable_basic!(i8);
impl_mappable_basic!(i16);
impl_mappable_basic!(i32);
impl_mappable_basic!(i64);
//NOTE: we should also cover reals here
impl_mappable_basic!(f32);
impl_mappable_basic!(f64);
