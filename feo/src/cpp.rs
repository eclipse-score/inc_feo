// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Provides a convenience macro creating ffi definitions and Rust glue code for C++ components

/// Create ffi definitions and Rust glue code for a C++ component  
///
/// Call: `cpp_activity!(name, library)`
///
/// Arguments:
/// - name: name prefix of C-functions exported by the C++ library
/// - step_params: List of parameters expected by the step function, use `()` if none.
/// - library: Name of the library containing the C-functions
///
/// The C++ code is expected to reside in the given library and provide C-linkable functions
/// named
/// - `<name>_create`,
/// - `<name>_free`
/// - `<name>_startup`,
/// - `<name>_step`,
/// - `<name>_shutdown`,
#[macro_export]
macro_rules! cpp_activity {
    ( $name:ident, $library:literal ) => {
        pub mod $name {
            use feo::activity::Activity;
            use feo::ids::ActivityId;
            use feo_cpp_macros::{make_fn, make_fn_call};
            use feo_tracing::{instrument, tracing};
            use std::ffi::c_void;

            // Safety: definition of external C functions belonging to C++ activity, to be reviewed
            #[link(name = $library)]
            unsafe extern "C" {
        make_fn!($name, _create, (activity_id: u64), (*mut c_void));
        make_fn!($name, _startup, (act_p: *mut c_void), ());
        make_fn!($name, _step, (act_p: *mut c_void), ());
        make_fn!($name, _shutdown, (act_p: *mut c_void), ());
        make_fn!($name, _free, (act_p: *mut c_void), ());
            }

            #[derive(Debug)]
            pub struct CppActivity {
                /// ID of the activity
                activity_id: ActivityId,
                /// Pointer to wrapped initialized C++ class instance
                cpp_activity: *mut c_void,
            }

            impl CppActivity {
                pub fn build(activity_id: ActivityId) -> Box<dyn Activity> {
                    // Create C++ activity in heap memory of C++
                    // Safety: Call of external C functions belonging to C++ activitiy, to be reviewed
                    let cpp_activity =
                        unsafe { make_fn_call!($name, _create, (activity_id.into())) };

                    Box::new(Self {
                        activity_id,
                        cpp_activity,
                    })
                }
            }

            impl Drop for CppActivity {
                fn drop(&mut self) {
                    // Free C++ activity in heap memory of C++
                    // Safety: Call of external C functions belonging to C++ activitiy, to be reviewed
                    unsafe { make_fn_call!($name, _free, (self.cpp_activity)) };
                }
            }

            impl Activity for CppActivity {
                fn id(&self) -> ActivityId {
                    self.activity_id
                }

                #[instrument(name = "startup")]
                fn startup(&mut self) {
                    // Safety: Call of external C functions belonging to C++ activitiy, to be reviewed
                    unsafe { make_fn_call!($name, _startup, (self.cpp_activity)) };
                }

                #[instrument(name = "step")]
                fn step(&mut self) {
                    // Safety: Call of external C functions belonging to C++ activitiy, to be reviewed
                    unsafe { make_fn_call!($name, _step, (self.cpp_activity)) };
                }

                #[instrument(name = "shutdown")]
                fn shutdown(&mut self) {
                    // Safety: Call of external C functions belonging to C++ activitiy, to be reviewed
                    unsafe { make_fn_call!($name, _shutdown, (self.cpp_activity)) };
                }
            }
        }
    };
}
