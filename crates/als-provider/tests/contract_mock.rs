//! MockProvider cũng phải qua contract — nếu mock fail thì mọi test dựa trên
//! mock ở orchestrator đều vô nghĩa.

use als_provider::{contract, MockProvider};

#[tokio::test]
async fn mock_passes_contract() {
    let provider = MockProvider::new();
    contract::run_all(&provider).await;
}
