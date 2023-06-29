pub(crate) struct Activities {
    #[allow(dead_code)] // Code is considered dead on non-Windows/linux
    pub(crate) activity_id: [u8; 16],                // Guid
    #[allow(dead_code)]
    pub(crate) parent_activity_id: Option<[u8; 16]>, // Guid
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

        let parent_activity_id = if parent_span_id == 0 {
            None
        } else {
            let mut activity_id: [u8; 16] = [0; 16];
            let (_, half) = activity_id.split_at_mut(8);
            half.copy_from_slice(&parent_span_id.to_le_bytes());

            Some(activity_id)
        };

        Activities {
            activity_id,
            parent_activity_id,
        }
    }
}
