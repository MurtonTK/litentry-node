#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// https://substrate.dev/docs/en/knowledgebase/runtime/frame

use sp_std::{prelude::*};
// use sp_arithmetic::traits::SaturatedConversion;

use frame_system::{
	ensure_signed, ensure_none,
	offchain::{CreateSignedTransaction, SubmitTransaction},
};
use frame_support::{
	debug, dispatch, decl_module, decl_storage, decl_event, decl_error,
	traits::Get, ensure, storage::IterableStorageMap,
};
use sp_core::crypto::KeyTypeId;
use simple_json::{self, json::JsonValue};

use sp_runtime::{
	transaction_validity::{
		ValidTransaction, InvalidTransaction, TransactionValidity, TransactionSource, TransactionLongevity,
	},
};
use sp_runtime::offchain::http;
use codec::Encode;

#[cfg(test)]
mod tests;

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"btc!");

// The link is ETHER_SCAN_PREFIX + Ethereum account + ETHER_SCAN_POSTFIX + ETHER_SCAN_TOKEN
pub const ETHER_SCAN_PREFIX: &str = "https://api.etherscan.io/api?module=account&action=balance&address=0x";
pub const ETHER_SCAN_POSTFIX: &str = "&tag=latest&apikey=";
pub const ETHER_SCAN_TOKEN: &str = "RF71W4Z2RDA7XQD6EN19NGB66C2QD9UPHB";

pub mod crypto {
	use super::KEY_TYPE;
	use sp_runtime::{
		app_crypto::{app_crypto, sr25519},
		traits::Verify,
	};
	use sp_core::sr25519::Signature as Sr25519Signature;
	app_crypto!(sr25519, KEY_TYPE);

	pub struct TestAuthId;
	impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Trait: frame_system::Trait + CreateSignedTransaction<Call<Self>> {
	// type AuthorityId: AppCrypto<Self::Public, Self::Signature>;

	/// Because this pallet emits events, it depends on the runtime's definition of an event.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	type Call: From<Call<Self>>;
}

// The pallet's runtime storage items.
// https://substrate.dev/docs/en/knowledgebase/runtime/storage
decl_storage! {
	// A unique name is used to ensure that the pallet's storage items are isolated.
	// This name may be updated, but each pallet in the runtime must use a unique name.
	// ---------------------------------vvvvvvvvvvvvvv
	trait Store for Module<T: Trait> as TemplateModule {
		// Learn more about declaring storage items:
		// https://substrate.dev/docs/en/knowledgebase/runtime/storage#declaring-storage-items
		Something get(fn something): Option<u32>;
		ClaimAccountSet get(fn query_account_set): map hasher(blake2_128_concat) T::AccountId => ();
	}
}

// Pallets use events to inform users when important changes are made.
// https://substrate.dev/docs/en/knowledgebase/runtime/events
decl_event!(
	pub enum Event<T> where	AccountId = <T as frame_system::Trait>::AccountId, {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		SomethingStored(u32, Option<AccountId>),
	}
);

// Errors inform users that something went wrong.
decl_error! {
	pub enum Error for Module<T: Trait> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}
}

// Dispatchable functions allows users to interact with the pallet and invoke state changes.
// These functions materialize as "extrinsics", which are often compared to transactions.
// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		// Errors must be initialized if they are used by the pallet.
		type Error = Error<T>;

		// Events must be initialized if they are used by the pallet.
		fn deposit_event() = default;

		#[weight = 10_000]
		pub fn asset_claim(origin,) -> dispatch::DispatchResult {
			let account = ensure_signed(origin)?;

			ensure!(!ClaimAccountSet::<T>::contains_key(&account), Error::<T>::StorageOverflow);

			<ClaimAccountSet<T>>::insert(&account, ());
			Ok(())
		}

		#[weight = 10_000]
		pub fn record_price(
			origin,
			// _block: T::BlockNumber,
			price: u32
		) -> dispatch::DispatchResult {
			// Ensuring this is an unsigned tx
			ensure_none(origin)?;
			Something::set(Some(price));
			// Spit out an event and Add to storage
			Self::deposit_event(RawEvent::SomethingStored(price, None));

			Ok(())
		}


		fn offchain_worker(block: T::BlockNumber) {
			// Get the all accounts who ask for asset claims
			let accounts: Vec<T::AccountId> = <ClaimAccountSet::<T>>::iter().map(|(k, v)| k).collect();
			// Remove all claimed accounts
			<ClaimAccountSet::<T>>::drain();
			
			// Get the Ethereum account from account linker interface
			let fixed_account: [u8; 20] = [0; 20];

			debug::info!("Hello World.");
			// Something::set(Some(block.saturated_into::<u32>()));
			let result = Self::fetch_etherscan(accounts);
			if let Err(e) = result {
				debug::info!("Hello World.{:?} ", e);
			}
		}

		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		#[weight = 10_000 + T::DbWeight::get().writes(1)]
		pub fn do_something(origin, something: u32) -> dispatch::DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// https://substrate.dev/docs/en/knowledgebase/runtime/origin
			let who = ensure_signed(origin)?;

			// Update storage.
			Something::put(something);

			// Emit an event.
			Self::deposit_event(RawEvent::SomethingStored(something, Some(who)));
			// Return a successful DispatchResult
			Ok(())
		}

		/// An example dispatchable that may throw a custom error.
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn cause_error(origin) -> dispatch::DispatchResult {
			let _who = ensure_signed(origin)?;

			// Read a value from storage.
			match Something::get() {
				// Return an error if the value has not been set.
				None => Err(Error::<T>::NoneValue)?,
				Some(old) => {
					// Increment the value read from storage; will error in the event of overflow.
					let new = old.checked_add(1).ok_or(Error::<T>::StorageOverflow)?;
					// Update the value in storage with the incremented result.
					Something::put(new);
					Ok(())
				},
			}
		}
	}
}

impl<T: Trait> Module<T> {
	fn fetch_github_info() -> Result<(), Error<T>> {
		let result = Self::fetch_json(b"https://api.coincap.io/v2/assets/bitcoin");

		let mut init = 10000;
		match result {
			Ok(_) => init = init + 1,
			Err(_) => init = init - 1,
		};
		
		let call = Call::record_price(init);
		SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
		.map_err(|_| {
			debug::error!("Failed in offchain_unsigned_tx");
			<Error<T>>::StorageOverflow
		})

	}

	fn fetch_etherscan(account_vec: Vec<T::AccountId>) ->  Result<(), Error<T>> {

		//let mut account_cat_str: String  = ETHER_SCAN_PREFIX;

    for _account in account_vec {
			//account_cat_str.push_str(""); //core::str::from_utf8(account);
		}

		//account_cat_str += (ETHER_SCAN_POSTFIX + ETHER_SCAN_TOKEN);

		//debug::info!("current url is {}", account_cat_str);

		let _result = Self::fetch_json(b"https://api.etherscan.io/api?module=account&action=balance&address=0x742d35Cc6634C0532925a3b844Bc454e4438f44e&tag=latest&apikey=RF71W4Z2RDA7XQD6EN19NGB66C2QD9UPHB");

		debug::info!("hi etherscan!");

		Ok(())
	}

	fn fetch_json<'a>(remote_url: &'a [u8]) -> Result<(), &'static str> {
		let remote_url_str = core::str::from_utf8(remote_url)
			.map_err(|_| "Error in converting remote_url to string")?;
	
		let pending = http::Request::get(remote_url_str).send()
			.map_err(|_| "Error in sending http GET request")?;
	
		let response = pending.wait()
			.map_err(|_| "Error in waiting http response back")?;
	
		if response.code != 200 {
			debug::warn!("Unexpected status code: {}", response.code);
			return Err("Non-200 status code returned from http request");
		}
	
		let json_result: Vec<u8> = response.body().collect::<Vec<u8>>();
	
		// print_bytes(&json_result);
	
		let _json_val: JsonValue = simple_json::parse_json(
			&core::str::from_utf8(&json_result).map_err(|_| "JSON result cannot convert to string")?)
			.map_err(|_| "JSON parsing error")?;

		debug::info!("Current JSON response is: \n {}",core::str::from_utf8(&json_result).unwrap());
	
		Ok(())
	}
}

#[allow(deprecated)]
impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
  type Call = Call<T>;

  #[allow(deprecated)]
  fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {

    match call {
      Call::record_price(price) => Ok(ValidTransaction {
        priority: 0,
        requires: vec![],
        provides: vec![(price).encode()],
        longevity: TransactionLongevity::max_value(),
        propagate: true,
      }),
    //   Call::record_agg_pp(block, sym, price) => Ok(ValidTransaction {
    //     priority: 0,
    //     requires: vec![],
    //     provides: vec![(block, sym, price).encode()],
    //     longevity: TransactionLongevity::max_value(),
    //     propagate: true,
    //   }),
      _ => InvalidTransaction::Call.into()
    }
  }
}