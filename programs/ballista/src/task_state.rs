use std::ptr;

use ballista_common::{logical_components::Value, schema::TaskDefinition};
use solana_program::{account_info::AccountInfo, instruction::AccountMeta};

pub struct TaskState<'info, 'a>
where
    'info: 'a,
{
    pub definition: &'a TaskDefinition,
    pub inputs: &'a [Value],
    pub cached_values: Vec<Option<Value>>,
    pub all_accounts: &'a [AccountInfo<'info>],
    pub account_meta_cache: CacheVec<AccountMeta>,
    pub account_info_cache: CacheVec<AccountInfo<'info>>,
    pub instruction_data_cache: Vec<u8>,
}

impl<'info, 'a> TaskState<'info, 'a> {
    pub fn purge_instruction_cache(&mut self) {
        self.account_meta_cache.clear();
        self.instruction_data_cache.clear();
        self.account_info_cache.clear();
    }
}

pub struct CacheVec<T> {
    data: Vec<T>,
    len: usize,
}

impl std::io::Write for CacheVec<u8> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.data.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.data.flush()
    }
}

impl<T> CacheVec<T> {
    // pub fn new(capacity: usize) -> Self {
    //     Self {
    //         data: Vec::with_capacity(capacity),
    //         len: 0,
    //     }
    // }

    // pub fn push(&mut self, item: T) {
    //     self.data.push(item);
    //     self.len += 1;
    // }

    pub fn to_vec(&self) -> Vec<T> {
        unsafe {
            Vec::from_raw_parts(
                self.data.as_ptr() as *mut T,
                self.data.len(),
                self.data.capacity(),
            )
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.len = 0;
    }

    // #[allow(clippy::uninit_vec)]
    #[allow(clippy::uninit_vec)]
    #[inline(always)]
    pub fn new(capacity: usize) -> Self {
        let data = Vec::with_capacity(capacity);
        Self { data, len: 0 }
    }

    #[inline(always)]
    pub fn push(&mut self, item: T) {
        unsafe {
            if self.len < self.data.len() {
                let ptr = self.data.as_mut_ptr().add(self.len);
                ptr::write(ptr, item);
            } else {
                // If capacity is exceeded, push to the vector (will allocate more memory)
                self.data.push(item);
            }
            self.len += 1;
        }
    }

    pub fn extend_from_slice(&mut self, items: &[T])
    where
        T: Clone,
    {
        self.data.extend_from_slice(items);
        self.len = self.data.len();
    }

    // #[inline(always)]
    // pub fn extend_from_slice(&mut self, items: &[T]) {
    //     let items_len = items.len();

    //     // Calculate the new total length after adding the items
    //     let total_len = self.len + items_len;

    //     // Ensure there's enough capacity in `self.data` to hold the new items
    //     if total_len > self.data.capacity() {
    //         // Reserve additional capacity
    //         self.data.reserve(total_len - self.data.capacity());
    //     }

    //     unsafe {
    //         let dst_ptr = self.data.as_mut_ptr().add(self.len);
    //         let src_ptr = items.as_ptr();

    //         // Copy items from `items` into `self.data`
    //         ptr::copy_nonoverlapping(src_ptr, dst_ptr, items_len);

    //         // Update `self.len` to reflect the new total length
    //         self.len = total_len;

    //         // If `self.data.len()` is less than `self.len`, update it
    //         if self.data.len() < self.len {
    //             self.data.set_len(self.len);
    //         }
    //     }
    // }

    // #[inline(always)]
    // pub fn clear(&mut self) {
    //     // Unsafe: Manually drop the elements to prevent memory leaks
    //     // unsafe {
    //     // for i in 0..self.len {
    //     //     ptr::drop_in_place(self.data.as_mut_ptr().add(i));
    //     // }
    //     self.len = 0;
    //     // }
    // }

    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        // Unsafe: Creating a slice from the raw parts
        unsafe { std::slice::from_raw_parts(self.data.as_ptr(), self.len) }
    }
}