use rscholar::db::{self, llm_providers};
use rscholar::llm::openai_compatible::{InterfaceType, OpenAiCompatibleProvider};
use rscholar::server::pipeline::SearchQueryPlan;

#[test]
fn openai_compatible_parses_chat_and_responses_shapes() {
    let chat = r#"{"choices":[{"message":{"content":"machine learning"}}]}"#;
    let responses_output_text = r#"{"output_text":"rock strength prediction"}"#;
    let responses_nested =
        r#"{"output":[{"content":[{"type":"output_text","text":"deep learning"}]}]}"#;

    assert_eq!(
        OpenAiCompatibleProvider::parse_chat_response(chat).unwrap(),
        "machine learning"
    );
    assert_eq!(
        OpenAiCompatibleProvider::parse_responses_response(responses_output_text).unwrap(),
        "rock strength prediction"
    );
    assert_eq!(
        OpenAiCompatibleProvider::parse_responses_response(responses_nested).unwrap(),
        "deep learning"
    );
}

#[test]
fn openai_compatible_builds_distinct_payloads() {
    let provider = OpenAiCompatibleProvider::new_for_test(
        "custom",
        InterfaceType::ChatCompletions,
        "https://example.com/v1/chat/completions",
        "demo-model",
        "test-key",
    )
    .unwrap();
    let payload = provider.build_payload_for_test("translate this");
    assert_eq!(payload["model"], "demo-model");
    assert!(payload.get("messages").is_some());

    let provider = OpenAiCompatibleProvider::new_for_test(
        "custom",
        InterfaceType::Responses,
        "https://example.com/v1/responses",
        "demo-model",
        "test-key",
    )
    .unwrap();
    let payload = provider.build_payload_for_test("translate this");
    assert_eq!(payload["model"], "demo-model");
    assert!(payload.get("input").is_some());
}

#[test]
fn openai_compatible_accepts_base_urls_and_normalizes_endpoint() {
    let chat_provider = OpenAiCompatibleProvider::new_for_test(
        "custom",
        InterfaceType::ChatCompletions,
        "https://example.com",
        "demo-model",
        "test-key",
    )
    .unwrap();
    assert_eq!(
        chat_provider.endpoint_for_test(),
        "https://example.com/v1/chat/completions"
    );

    let responses_provider = OpenAiCompatibleProvider::new_for_test(
        "custom",
        InterfaceType::Responses,
        "https://example.com/v1",
        "demo-model",
        "test-key",
    )
    .unwrap();
    assert_eq!(
        responses_provider.endpoint_for_test(),
        "https://example.com/v1/responses"
    );
}

#[test]
fn llm_provider_storage_masks_and_preserves_api_keys() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    db::schema::init_tables(&conn).unwrap();

    llm_providers::upsert(
        &conn,
        llm_providers::UpsertProvider {
            name: "custom".to_string(),
            enabled: true,
            interface_type: "responses".to_string(),
            endpoint: "https://example.com/v1/responses".to_string(),
            model: "demo-model".to_string(),
            api_key: Some("sk-secret".to_string()),
            order: 7,
        },
    )
    .unwrap();

    llm_providers::upsert(
        &conn,
        llm_providers::UpsertProvider {
            name: "custom".to_string(),
            enabled: false,
            interface_type: "responses".to_string(),
            endpoint: "https://example.com/v1/responses".to_string(),
            model: "demo-model-2".to_string(),
            api_key: None,
            order: 8,
        },
    )
    .unwrap();

    let public = llm_providers::list_public(&conn).unwrap();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].name, "custom");
    assert!(!public[0].enabled);
    assert!(public[0].api_key_set);
    assert_eq!(public[0].order, 8);

    let runtime = llm_providers::list_enabled_runtime(&conn).unwrap();
    assert!(runtime.is_empty());

    let stored = llm_providers::get_runtime(&conn, "custom")
        .unwrap()
        .unwrap();
    assert_eq!(stored.api_key, "sk-secret");
    assert_eq!(stored.model, "demo-model-2");
}

#[test]
fn search_query_plan_detects_cjk_and_keeps_original_with_translated_terms() {
    let plan = SearchQueryPlan::new(
        "机器学习 岩石强度预测",
        "machine learning rock strength prediction",
        vec![
            "deep learning".to_string(),
            "uniaxial compressive strength".to_string(),
        ],
    );

    assert!(plan.is_cjk_query);
    assert_eq!(plan.original_keyword, "机器学习 岩石强度预测");
    assert_eq!(
        plan.primary_keyword(),
        "machine learning rock strength prediction"
    );
    assert_eq!(
        plan.fallback_keywords(),
        vec!["机器学习 岩石强度预测".to_string()]
    );
    assert!(plan.openalex_terms().contains(&"deep learning".to_string()));
}

#[test]
fn cjk_relevance_tokenization_rewards_chinese_overlap() {
    let exact = rscholar::server::pipeline::test_support::local_relevance_score_for_test(
        "机器学习 岩石强度预测",
        Some("关注岩石强度和机器学习预测方法"),
        "基于机器学习的岩石强度预测",
        "本文研究岩石强度预测方法。",
    );
    let weak = rscholar::server::pipeline::test_support::local_relevance_score_for_test(
        "机器学习 岩石强度预测",
        Some("关注岩石强度和机器学习预测方法"),
        "香蕉根系形态观察",
        "本文研究植物生长。",
    );

    assert!(exact.score > weak.score);
}
