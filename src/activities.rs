use std::io::Write;
use std::{io::Cursor, mem::MaybeUninit};

pub(crate) struct Activities {
    pub(crate) span_id: [u8; 16],                    // Hex string
    pub(crate) activity_id: [u8; 16],                // Guid
    pub(crate) parent_activity_id: Option<[u8; 16]>, // Guid
    pub(crate) parent_span_id: [u8; 16],             // Hex string
}

impl Default for Activities {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

impl Activities {
    #[allow(invalid_value)]
    pub(crate) fn generate(span_id: u64, parent_span_id: u64) -> Activities {
        let mut activity_id: [u8; 16] = [0; 16];
        let (_, half) = activity_id.split_at_mut(8);
        half.copy_from_slice(&span_id.to_le_bytes());

        let (parent_activity_id, parent_span_name) = if parent_span_id == 0 {
            (None, [0; 16])
        } else {
            let mut buf: [u8; 16] = unsafe { MaybeUninit::uninit().assume_init() };
            let mut cur = Cursor::new(&mut buf[..]);
            write!(&mut cur, "{:16x}", span_id).expect("!write");

            let mut activity_id: [u8; 16] = [0; 16];
            let (_, half) = activity_id.split_at_mut(8);
            half.copy_from_slice(&parent_span_id.to_le_bytes());

            (Some(activity_id), buf)
        };

        let mut buf: [u8; 16] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut cur = Cursor::new(&mut buf[..]);
        write!(&mut cur, "{:16x}", span_id).expect("!write");

        Activities {
            span_id: buf,
            activity_id,
            parent_activity_id,
            parent_span_id: parent_span_name,
        }
    }
}
