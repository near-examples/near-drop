//! Run with: cargo test --test main measure_claim_gas -- --ignored --nocapture
//! Searches the minimum prepaid gas at 1 TGas resolution with current allocations.
use crate::init::{init, init_failing_ft_contract, init_ft_contract, init_nft_contract};
use crate::utils::{CLAIM_GAS, INITIAL_CONTRACT_BALANCE, ONE_HUNDRED_TGAS};
use near_sdk::{
    serde_json::{json, Value},
    Gas, NearToken,
};
use near_workspaces::{
    network::Sandbox,
    types::{KeyType, SecretKey},
    Account, Contract, Worker,
};

struct Fixture {
    worker: Worker<Sandbox>,
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
                .create_subaccount(&format!("recipient{}", self.attempt))
                .initial_balance(NearToken::from_millinear(20))
                .transact()
                .await?
                .into_result()?
        } else {
            self.recipient.clone()
        };
        let keys: Vec<_> = (0..count)
            .map(|_| SecretKey::from_random(KeyType::ED25519))
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
        let cost: NearToken = self
            .contract
            .view(self.contract.id(), "get_drop_cost")
            .args_json(json!({"funder": self.creator.id(), "drop": input}))
            .await?
            .json()?;
        let created = self
            .creator
            .call(self.contract.id(), "create_drop")
            .args_json(json!({"drop": input}))
            .deposit(cost)
            .gas(ONE_HUNDRED_TGAS)
            .transact()
            .await?;
        anyhow::ensure!(created.is_success(), "create failed: {created:?}");
        let id: u32 = created.json()?;
        let token_id = format!("{:0>128}", self.attempt);
        if kind == "FT" {
            let funded = self.creator.call(token.id(), "ft_transfer_call")
                .args_json(json!({"receiver_id": self.contract.id(), "amount": count.to_string(), "msg": id.to_string()}))
                .deposit(NearToken::from_yoctonear(1)).gas(ONE_HUNDRED_TGAS).transact().await?;
            anyhow::ensure!(funded.is_success(), "funding failed: {funded:?}");
        } else if kind == "NFT" {
            let mint = self.creator.call(self.nft.id(), "nft_mint")
                .args_json(json!({"token_id": token_id, "token_owner_id": self.creator.id(), "token_metadata": {"copies": 1}}))
                .deposit(NearToken::from_millinear(100)).gas(ONE_HUNDRED_TGAS).transact().await?;
            anyhow::ensure!(mint.is_success(), "mint failed: {mint:?}");
            let approve = self.creator.call(self.nft.id(), "nft_approve")
                .args_json(json!({"token_id": token_id, "account_id": self.contract.id(), "msg": id.to_string()}))
                .deposit(NearToken::from_millinear(10)).gas(ONE_HUNDRED_TGAS).transact().await?;
            anyhow::ensure!(approve.is_success(), "approval failed: {approve:?}");
            if fails {
                self.creator
                    .call(self.nft.id(), "nft_revoke")
                    .args_json(json!({"token_id": token_id, "account_id": self.contract.id()}))
                    .deposit(NearToken::from_yoctonear(1))
                    .gas(ONE_HUNDRED_TGAS)
                    .transact()
                    .await?
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
        let claimed = claimer
            .call(self.contract.id(), "claim_for")
            .args_json(json!({"account_id": account_id}))
            .gas(Gas::from_tgas(gas))
            .transact()
            .await?;
        let errors = format!("{:?}", claimed.receipt_failures()).to_lowercase();
        let burnt = claimed.total_gas_burnt.as_gas() as f64 / 1e12;
        let no_failures = claimed.receipt_failures().is_empty();
        let worked = claimed.json::<bool>().ok() == Some(!fails)
            && !errors.contains("gasexceeded")
            && !errors.contains("gaslimitexceeded")
            && (fails || no_failures);
        if worked {
            anyhow::ensure!(
                self.contract
                    .view_access_key(&keys[0].public_key())
                    .await
                    .is_err(),
                "key was not consumed"
            );
            if kind == "NFT" && !fails {
                let nft: Value = self
                    .nft
                    .view("nft_token")
                    .args_json(json!({"token_id": token_id}))
                    .await?
                    .json()?;
                anyhow::ensure!(
                    nft["owner_id"] == recipient.id().as_str(),
                    "NFT was not delivered"
                );
            }
        }
        println!("{kind} keys={count} registered={registered} failure={fails} prepaid={gas} TGas burnt={:.3} TGas worked={worked}", burnt);
        // A low-gas attempt may leave the drop intact, or consume just one key.
        if self
            .contract
            .view(self.contract.id(), "get_drop_by_id")
            .args_json(json!({"drop_id": id}))
            .await
            .is_ok()
        {
            let _ = self
                .creator
                .call(self.contract.id(), "delete_drop")
                .args_json(json!({"drop_id": id}))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?;
            anyhow::ensure!(
                self.contract
                    .view(self.contract.id(), "get_drop_by_id")
                    .args_json(json!({"drop_id": id}))
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
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let (contract, creator, recipient) = init(&root, INITIAL_CONTRACT_BALANCE).await?;
    root.transfer_near(creator.id(), NearToken::from_near(30))
        .await?
        .into_result()?;
    let ft = init_ft_contract(&worker, &creator).await?;
    let failing_ft = init_failing_ft_contract(&worker, &creator).await?;
    for token in [&ft, &failing_ft] {
        for account in [&contract, &recipient] {
            creator
                .call(token.id(), "storage_deposit")
                .args_json(json!({"account_id": account.id()}))
                .deposit(NearToken::from_millinear(10))
                .gas(ONE_HUNDRED_TGAS)
                .transact()
                .await?
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
