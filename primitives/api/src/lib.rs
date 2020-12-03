
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unnecessary_mut_passed)]

use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
	pub trait Token {
		/// Returns the version of the runtime.
		fn version_ext(a: Vec<u8>);
		
	}
}
