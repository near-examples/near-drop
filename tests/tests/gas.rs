//! Run with: cargo test --test main measure_claim_gas -- --ignored --nocapture
//! Searches the minimum prepaid gas at 1 TGas resolution with current allocations.
use crate::init::{init, init_failing_ft_contract, init_ft_contract, init_nft_contract};
use crate::init::{TestAccount as Account, TestAccount as Contract};
use crate::utils::{CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};
use near_sdk::{
    serde_json::{json, Value},
    Gas, NearToken,
};

struct Fixture {
    worker: crate::init::TestSandbox,
    root: Account,
    contract: Account,
    creator: Account,
    recipient: Account,
    ft: Contract,
    failing_ft: Contract,
    nft: Contract,
    attempt: u32,
}

impl Fixture {
    async fn try_claim(
        &mut self,
        kind: &str,
        count: usize,
        registered: bool,
        fails: bool,
        gas: u64,
    ) -> anyhow::Result<bool> {
        self.attempt += 1;
        let recipient = if kind == "FT" && !registered {
            self.root
                .create_subaccount(
                    &format!("recipient{}", self.attempt),
                    NearToken::from_millinear(20),
                )
                .await?
        } else {
            self.recipient.clone()
        };
        let keys: Vec<_> = (0..count)
            .map(|_| near_api::signer::generate_secret_key().expect("generate test key"))
            .collect();
        let public_keys: Vec<_> = keys.iter().map(|key| key.public_key()).collect();
        let token = if fails { &self.failing_ft } else { &self.ft };
        let input = match kind {
            "NEAR" => {
                json!({"NEAR": {"public_keys": public_keys, "amount_per_drop": NearToken::from_millinear(1)}})
            }
            "FT" => {
                json!({"FT": {"public_keys": public_keys, "ft_contract": token.id(), "amount_per_drop": "1"}})
            }
            "NFT" => json!({"NFT": {"public_key": public_keys[0], "nft_contract": self.nft.id()}}),
            _ => unreachable!(),
        };
        let cost: NearToken = near_api::Contract(self.contract.id().clone())
            .call_function(
                "get_drop_cost",
                json!({"funder": self.creator.id(), "drop": input}),
            )
            .read_only::<_>()
            .fetch_from(&self.contract.network)
            .await?
            .data;
        let created = near_api::Contract(self.contract.id().clone())
            .call_function("create_drop", json!({"drop": input}))
            .transaction()
            .deposit(cost)
            .gas(ONE_HUNDRED_TGAS)
            .with_signer(self.creator.id().clone(), self.creator.signer.clone())
            .send_to(&self.creator.network)
            .await?
            .into_full()
            .expect("final transaction outcome");
        anyhow::ensure!(created.is_success(), "create failed: {created:?}");
        let id: u32 = created.json()?;
        let token_id = format!("{:0>128}", self.attempt);
        if kind == "FT" {
            let funded = near_api::Contract(token.id().clone()).call_function("ft_transfer_call", json!({"receiver_id": self.contract.id(), "amount": count.to_string(), "msg": id.to_string()})).transaction()
                .deposit(NearToken::from_yoctonear(1)).gas(ONE_HUNDRED_TGAS).with_signer(self.creator.id().clone(), self.creator.signer.clone())
        .send_to(&self.creator.network).await?.into_full().expect("final transaction outcome");
            anyhow::ensure!(funded.is_success(), "funding failed: {funded:?}");
        } else if kind == "NFT" {
            let mint = near_api::Contract(self.nft.id().clone()).call_function("nft_mint", json!({"token_id": token_id, "token_owner_id": self.creator.id(), "token_metadata": {"copies": 1}})).transaction()
                .deposit(NearToken::from_millinear(100)).gas(ONE_HUNDRED_TGAS).with_signer(self.creator.id().clone(), self.creator.signer.clone())
        .send_to(&self.creator.network).await?.into_full().expect("final transaction outcome");
            anyhow::ensure!(mint.is_success(), "mint failed: {mint:?}");
            let approve = near_api::Contract(self.nft.id().clone()).call_function("nft_approve", json!({"token_id": token_id, "account_id": self.contract.id(), "msg": id.to_string()})).transaction()
                .deposit(NearToken::from_millinear(10)).gas(ONE_HUNDRED_TGAS).with_signer(self.creator.id().clone(), self.creator.signer.clone())
        .send_to(&self.creator.network).await?.into_full().expect("final transaction outcome");
            anyhow::ensure!(approve.is_success(), "approval failed: {approve:?}");
            if fails {
                near_api::Contract(self.nft.id().clone())
                    .call_function(
                        "nft_revoke",
                        json!({"token_id": token_id, "account_id": self.contract.id()}),
                    )
                    .transaction()
                    .deposit(NearToken::from_yoctonear(1))
                    .gas(ONE_HUNDRED_TGAS)
                    .with_signer(self.creator.id().clone(), self.creator.signer.clone())
                    .send_to(&self.creator.network)
                    .await?
                    .into_full()
                    .expect("final transaction outcome")
                    .into_result()?;
            }
        }
        let account_id = if kind == "NEAR" && fails {
            format!("missing.{}", self.root.id())
        } else {
            recipient.id().to_string()
        };
        let claimer =
            Account::from_secret_key(self.contract.id().clone(), keys[0].clone(), &self.worker);
        let claimed = near_api::Contract(self.contract.id().clone())
            .call_function("claim_for", json!({"account_id": account_id}))
            .transaction()
            .gas(Gas::from_tgas(gas))
            .with_signer(claimer.id().clone(), claimer.signer.clone())
            .send_to(&claimer.network)
            .await?
            .into_full()
            .expect("final transaction outcome");
        let errors = format!("{:?}", claimed.receipt_failures()).to_lowercase();
        let burnt = claimed.total_gas_burnt.as_gas() as f64 / 1e12;
        let no_failures = claimed.receipt_failures().is_empty();
        let worked = claimed.json::<bool>().ok() == Some(!fails)
            && !errors.contains("gasexceeded")
            && !errors.contains("gaslimitexceeded")
            && (fails || no_failures);
        if worked {
            anyhow::ensure!(
                near_api::Account(self.contract.id().clone())
                    .access_key(keys[0].public_key().clone())
                    .fetch_from(&self.contract.network)
                    .await
                    .is_err(),
                "key was not consumed"
            );
            if kind == "NFT" && !fails {
                let nft: Value = near_api::Contract(self.nft.id().clone())
                    .call_function("nft_token", json!({"token_id": token_id}))
                    .read_only::<_>()
                    .fetch_from(&self.nft.network)
                    .await?
                    .data;
                anyhow::ensure!(
                    nft["owner_id"] == recipient.id().as_str(),
                    "NFT was not delivered"
                );
            }
        }
        println!("{kind} keys={count} registered={registered} failure={fails} prepaid={gas} TGas burnt={:.3} TGas worked={worked}", burnt);
        // A low-gas attempt may leave the drop intact, or consume just one key.
        if near_api::Contract(self.contract.id().clone())
            .call_function("get_drop_by_id", json!({"drop_id": id}))
            .read_only::<serde_json::Value>()
            .fetch_from(&self.contract.network)
            .await
            .is_ok()
        {
            let _ = near_api::Contract(self.contract.id().clone())
                .call_function("delete_drop", json!({"drop_id": id}))
                .transaction()
                .gas(ONE_HUNDRED_TGAS)
                .with_signer(self.creator.id().clone(), self.creator.signer.clone())
                .send_to(&self.creator.network)
                .await?
                .into_full()
                .expect("final transaction outcome");
            anyhow::ensure!(
                near_api::Contract(self.contract.id().clone())
                    .call_function("get_drop_by_id", json!({"drop_id": id}))
                    .read_only::<serde_json::Value>()
                    .fetch_from(&self.contract.network)
                    .await
                    .is_err(),
                "cleanup did not remove drop"
            );
        }
        Ok(worked)
    }
}

#[tokio::test]
#[ignore = "gas benchmark creates fresh drops and searches prepaid gas limits"]
async fn measure_claim_gas() -> anyhow::Result<()> {
    let worker = crate::init::sandbox().await?;
    let root = worker.root_account()?;
    let (contract, creator, recipient) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    near_api::Tokens::account(root.id().clone())
        .send_to(creator.id().clone())
        .near(NearToken::from_near(30))
        .with_signer(root.signer.clone())
        .send_to(&root.network)
        .await?
        .into_full()
        .expect("final transaction outcome")
        .into_result()?;
    let ft = init_ft_contract(&worker, &creator).await?;
    let failing_ft = init_failing_ft_contract(&worker, &creator).await?;
    for token in [&ft, &failing_ft] {
        for account in [&contract, &recipient] {
            near_api::Contract(token.id().clone())
                .call_function("storage_deposit", json!({"account_id": account.id()}))
                .transaction()
                .deposit(NearToken::from_millinear(10))
                .gas(ONE_HUNDRED_TGAS)
                .with_signer(creator.id().clone(), creator.signer.clone())
                .send_to(&creator.network)
                .await?
                .into_full()
                .expect("final transaction outcome")
                .into_result()?;
        }
    }
    let (nft, _) = init_nft_contract(&worker, &creator).await?;
    let mut fixture = Fixture {
        worker,
        root,
        contract,
        creator,
        recipient,
        ft,
        failing_ft,
        nft,
        attempt: 0,
    };
    for (kind, count, registered, fails) in [
        ("NEAR", 1, false, false),
        ("NEAR", 10, false, false),
        ("NEAR", 1, false, true),
        ("FT", 1, false, false),
        ("FT", 10, false, false),
        ("FT", 10, true, false),
        ("FT", 1, false, true),
        ("NFT", 1, false, false),
        ("NFT", 1, false, true),
    ] {
        let (mut low, mut high) = (0, CLAIM_GAS.as_tgas());
        anyhow::ensure!(
            fixture
                .try_claim(kind, count, registered, fails, high)
                .await?,
            "configured claim gas baseline failed"
        );
        while high - low > 1 {
            let mid = (low + high) / 2;
            if fixture
                .try_claim(kind, count, registered, fails, mid)
                .await?
            {
                high = mid;
            } else {
                low = mid;
            }
        }
        println!("MINIMUM {kind} keys={count} registered={registered} failure={fails}: {high} TGas (fails at {low})");
    }
    Ok(())
}
