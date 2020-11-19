#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{prelude::*};
use frame_system::{
	ensure_signed, ensure_none,
	offchain::{CreateSignedTransaction, SubmitTransaction},
};
use frame_support::{
	debug, dispatch, decl_module, decl_storage, decl_event, decl_error,
	ensure, storage::IterableStorageMap,
};
use sp_core::crypto::KeyTypeId;
use lite_json::{self, json::JsonValue};

use sp_runtime::{
	transaction_validity::{
		ValidTransaction, InvalidTransaction, TransactionValidity, TransactionSource, TransactionLongevity,
	},
};
use sp_runtime::offchain::http;
use codec::Encode;
use account_linker::EthereumLink;

#[cfg(test)]
mod tests;

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ocw!");

// https://api.etherscan.io/api?module=account&action=balancemulti&address=0x742d35Cc6634C0532925a3b844Bc454e4438f44e,0x742d35Cc6634C0532925a3b844Bc454e4438f44e&tag=latest&apikey=RF71W4Z2RDA7XQD6EN19NGB66C2QD9UPHB
// The link is ETHER_SCAN_PREFIX + 1st Ethereum account + ETHER_SCAN_DELIMITER + 2nd Ethereum account + ... + ETHER_SCAN_POSTFIX + ETHER_SCAN_TOKEN
pub const ETHER_SCAN_PREFIX: &str = "https://api.etherscan.io/api?module=account&action=balancemulti&address=0x";
pub const ETHER_SCAN_DELIMITER: &str = ",0x";
pub const ETHER_SCAN_POSTFIX: &str = "&tag=latest&apikey=";
pub const ETHER_SCAN_TOKEN: &str = "RF71W4Z2RDA7XQD6EN19NGB66C2QD9UPHB";

pub const SAMPLE_ACCOUNT: &str = "742d35Cc6634C0532925a3b844Bc454e4438f44e";

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

pub trait Trait: frame_system::Trait + account_linker::Trait + CreateSignedTransaction<Call<Self>> {
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	type Call: From<Call<Self>>;
}

decl_storage! {
	trait Store for Module<T: Trait> as OffchainWorkerModule {
		TotalClaims get(fn total_claims): u64;
		ClaimAccountSet get(fn query_account_set): map hasher(blake2_128_concat) T::AccountId => ();
		AccountBalance get(fn account_balance): map hasher(blake2_128_concat) T::AccountId => u64;
	}
}

decl_event!(
	pub enum Event<T> where	AccountId = <T as frame_system::Trait>::AccountId, {
		BalanceGot(AccountId, u64),
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
		pub fn record_balance(
			origin,
			account: T::AccountId,
			price: u64
		) -> dispatch::DispatchResult {
			// Ensuring this is an unsigned tx
			ensure_none(origin)?;
			// Record the total claims processed
			TotalClaims::put(Self::total_claims() + 1);
			// Spit out an event and Add to storage
			Self::deposit_event(RawEvent::BalanceGot(account, price));

			Ok(())
		}

		// Trigger by offchain framework in each block
		fn offchain_worker(block: T::BlockNumber) {
			// Get the all accounts who ask for asset claims
			let accounts: Vec<T::AccountId> = <ClaimAccountSet::<T>>::iter().map(|(k, _)| k).collect();
			// Remove all claimed accounts
			<ClaimAccountSet::<T>>::drain();

			debug::info!("Hello Offchain Worker.");
			Self::fetch_etherscan(accounts);
		}
	}
}

impl<T: Trait> Module<T> {
	// Fetch all claimed accounts
	fn fetch_etherscan(account_vec: Vec<T::AccountId>) ->  Result<(), Error<T>> {
		for (_, account) in account_vec.iter().enumerate() {
			Self::fetch_etherscan_account(account);
		}
		Ok(())
	}

	// fetch an account
	fn fetch_etherscan_account(account: &T::AccountId) ->  Result<(), Error<T>> {
		// Get all ethereum accounts linked to Litentry		
		let eth_accounts = <account_linker::EthereumLink<T>>::get(account);

		// Return if no ethereum account linked
		if eth_accounts.len() == 0 {
			return Ok(())
		}

		// Compose the web link
		let mut link: Vec<u8> = Vec::new();
		link.extend(ETHER_SCAN_PREFIX.as_bytes());

		for (i, eth_account) in eth_accounts.iter().enumerate() {
			// Append delimiter if there are more than one accounts in the account_vec
			if i >=1 {
				link.extend(eth_account);
			};
			link.extend(SAMPLE_ACCOUNT.as_bytes());
		}
		link.extend(ETHER_SCAN_POSTFIX.as_bytes());
		link.extend(ETHER_SCAN_TOKEN.as_bytes());

		// Get the json
		let result = Self::fetch_json(&link[..]).map_err(|_| Error::<T>::StorageOverflow)?;
		
		let response = sp_std::str::from_utf8(&result).map_err(|_| Error::<T>::StorageOverflow)?;
		let balances = Self::parse_multi_balances(response);

		match balances {
			Some(data) => {
				let mut total_balance: u64 = 0;
				for item in data {
					let balance = Self::chars_to_u64(item).map_err(|_| Error::<T>::StorageOverflow)?;
					total_balance = total_balance + balance;
				}
				let call = Call::record_balance(account.clone(), total_balance);
				let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
				.map_err(|_| {
					debug::error!("Failed in offchain_unsigned_tx");
					<Error<T>>::StorageOverflow
				});
			},
			None => (),
		}
		Ok(())
	}

	fn fetch_json<'a>(remote_url: &'a [u8]) -> Result<Vec<u8>, &'static str> {
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
		
		let balance =
			core::str::from_utf8(&json_result).map_err(|_| "JSON result cannot convert to string")?;
	
		Ok(balance.as_bytes().to_vec())
	}

	fn parse_multi_balances(price_str: &str) -> Option<Vec<Vec<char>>> {
		// {
		// "status": "1",
		// "message": "OK",
		// "result": 
		//   [
		//     {"account":"0x742d35Cc6634C0532925a3b844Bc454e4438f44e","balance":"3804372455842738500000001"},
		//     {"account":"0xBE0eB53F46cd790Cd13851d5EFf43D12404d33E8","balance":"2571179226430511381996287"}
		//   ]
		// }
		let val = lite_json::parse_json(price_str);
		let balance_chars = "balance".chars();
		let balances = val.ok().and_then(|v| match v {
			JsonValue::Object(obj) => {
				let mut chars = "result".chars();
				obj.into_iter()
					.find(|(k, _)| k.iter().all(|k| Some(*k) == chars.next()))
					.and_then(|v| match v.1 {
						JsonValue::Array(res_array) => {
							// Get the balance list 
							let mut balance_vec: Vec<Vec<char>> = Vec::new();
							for element in res_array {
								match element {
									JsonValue::Object(element_vec) => {
										for pair in element_vec {											
											match pair.0 {
												// Match balance key
												balance_chars => {
													// Match balance value
													match pair.1 {
														// Put balance into vector
														JsonValue::String(balance) => balance_vec.push(balance),
														_ => (), 
													}
												},
												_ => (),
											}
										};
									},
									_ => ()
								}
							}
							Some(balance_vec)
						},
						_ => None,
					})
			},
			_ => None
		})?;

		Some(balances)
	}

	fn parse_balance(price_str: &str) -> Option<Vec<char>> {
		// {
		// "status": "1",
		// "message": "OK",
		// "result": "3795858430482738500000001"
		// }
		let val = lite_json::parse_json(price_str);
		let balance = val.ok().and_then(|v| match v {
			JsonValue::Object(obj) => {
				let mut chars = "result".chars();
				obj.into_iter()
					.find(|(k, _)| k.iter().all(|k| Some(*k) == chars.next()))
					.and_then(|v| match v.1 {
						JsonValue::String(balance) => Some(balance),
						_ => None,
					})
			},
			_ => None
		})?;
		Some(balance)
	}

	fn chars_to_u64(vec: Vec<char>) -> Result<u64, &'static str> {
		let mut result: u64 = 0;
		for item in vec {
			let n = item.to_digit(10);
			match n {
				Some(i) => {
					let i_64 = i as u64; 
					result = result * 10 + i_64;
					if result < i_64 {
						return Err("Wrong u64 balance data format");
					}
				},
				None => return Err("Wrong u64 balance data format"),
			}
		}
		return Ok(result)
	}
}

#[allow(deprecated)]
impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	#[allow(deprecated)]
	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {

		match call {
		Call::record_balance(account, price) => Ok(ValidTransaction {
			priority: 0,
			requires: vec![],
			provides: vec![(account, price).encode()],
			longevity: TransactionLongevity::max_value(),
			propagate: true,
		}),
		_ => InvalidTransaction::Call.into()
		}
	}
}