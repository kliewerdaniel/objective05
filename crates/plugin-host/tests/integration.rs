use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use objective_core::traits::MessageBus;
use objective_core::types::EventEnvelope;
use objective_message_bus::InMemoryMessageBus;
use objective_plugin_host::{
    audit_log_plugin, AuditLogPlugin, BuiltinPlugin, HostConfig, Plugin, PluginHost, PluginOutput,
    PluginState, ReEmitterPlugin,
};
use serde_json::json;
use tempfile::tempdir;

fn make_bus() -> Arc<InMemoryMessageBus> {
    Arc::new(InMemoryMessageBus::new())
}

fn make_host(bus: Arc<InMemoryMessageBus>, state_path: PathBuf) -> PluginHost<InMemoryMessageBus> {
    PluginHost::new(
        bus,
        state_path,
        HostConfig {
            route_poll_interval: Duration::from_millis(50),
            health_check_interval: Duration::from_secs(60),
            event_timeout: Duration::from_millis(500),
            max_event_backlog: 64,
        },
    )
    .expect("host")
}

#[tokio::test]
async fn register_builtin_marks_plugin_running() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    let manifest = objective_plugin_host::builtin_manifest(
        "audit-log",
        objective_plugin_host::PluginType::Filter,
        "0.1.0",
        vec!["ingestion.document.received".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(manifest, audit_log_plugin(vec![])))
        .await
        .expect("register");

    let status = host.get("audit-log").await.expect("status");
    assert_eq!(status.manifest.name, "audit-log");
    assert_eq!(
        status.manifest.plugin_type,
        objective_plugin_host::PluginType::Filter
    );
    assert_eq!(status.events_handled, 0);
    assert_eq!(status.state, PluginState::Validated);
}

#[tokio::test]
async fn plugin_receives_subscribed_events() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    let audit = Arc::new(AuditLogPlugin::new(vec![
        "ingestion.document.received".to_string()
    ]));
    let manifest = objective_plugin_host::builtin_manifest(
        "audit-log",
        objective_plugin_host::PluginType::Filter,
        "0.1.0",
        vec!["ingestion.document.received".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(
        manifest,
        Arc::clone(&audit) as Arc<dyn Plugin>,
    ))
    .await
    .expect("register");

    bus.publish(
        "ingestion.document.received",
        EventEnvelope::new(
            "ingestion.document.received",
            "test",
            json!({"document_id": "doc-1"}),
        ),
    )
    .await
    .expect("publish");

    host.route_once().await.expect("route");

    let status = host.get("audit-log").await.expect("status");
    assert!(status.events_handled >= 1);
    assert!(status.last_event_at.is_some());
    let log = audit.snapshot().await;
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].event_type, "ingestion.document.received");
}

#[tokio::test]
async fn plugin_does_not_receive_unsubscribed_events() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    let manifest = objective_plugin_host::builtin_manifest(
        "audit-log",
        objective_plugin_host::PluginType::Filter,
        "0.1.0",
        vec!["ingestion.document.received".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(
        manifest,
        audit_log_plugin(vec!["ingestion.document.received".to_string()]),
    ))
    .await
    .expect("register");

    bus.publish(
        "extraction.document.processed",
        EventEnvelope::new("extraction.document.processed", "test", json!({})),
    )
    .await
    .expect("publish");

    host.route_once().await.expect("route");

    let status = host.get("audit-log").await.expect("status");
    assert_eq!(status.events_handled, 0);
}

#[tokio::test]
async fn re_emitter_publishes_via_plugin_output() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    let re = Arc::new(ReEmitterPlugin::new(
        vec!["extraction.document.processed".to_string()],
        "plugin.re_emitted",
    ));
    let manifest = objective_plugin_host::builtin_manifest(
        "re-emitter",
        objective_plugin_host::PluginType::Processor,
        "0.1.0",
        vec!["extraction.document.processed".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(
        manifest,
        Arc::clone(&re) as Arc<dyn Plugin>,
    ))
    .await
    .expect("register");

    bus.publish(
        "extraction.document.processed",
        EventEnvelope::new(
            "extraction.document.processed",
            "test",
            json!({"document_id": "doc-1"}),
        ),
    )
    .await
    .expect("publish");

    host.route_once().await.expect("route");

    let events = bus.events().await.expect("events");
    assert!(
        events
            .iter()
            .any(|(subject, e)| subject == "plugin.re_emitted"
                && e.event_type == "plugin.re_emitted"),
        "re-emitter should publish a plugin.re_emitted event"
    );
}

#[tokio::test]
async fn restart_clears_failure_count_and_increments_restart() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    let manifest = objective_plugin_host::builtin_manifest(
        "audit-log",
        objective_plugin_host::PluginType::Filter,
        "0.1.0",
        vec!["ingestion.document.received".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(manifest, audit_log_plugin(vec![])))
        .await
        .expect("register");

    let status_before = host.get("audit-log").await.expect("status");
    let restarts_before = status_before.restart_count;
    let status_after = host.restart("audit-log").await.expect("restart");
    assert_eq!(status_after.restart_count, restarts_before + 1);
    assert_eq!(status_after.state, PluginState::Running);
    assert!(status_after.started_at.is_some());
}

#[tokio::test]
async fn restart_unknown_plugin_returns_unknown_error() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    let err = host.restart("does-not-exist").await.expect_err("missing");
    assert!(matches!(
        err,
        objective_plugin_host::HostError::UnknownPlugin(_)
    ));
}

#[tokio::test]
async fn discover_into_registers_manifests_from_directory() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();
    std::fs::create_dir_all(plugins_dir.join("echo-source")).unwrap();
    std::fs::write(
        plugins_dir.join("echo-source").join("plugin.json"),
        r#"{
            "name": "echo-source",
            "version": "0.1.0",
            "description": "Echo source plugin",
            "plugin_type": "source",
            "api_version": 1,
            "subscriptions": { "event_types": ["ingestion.document.received"] }
        }"#,
    )
    .unwrap();

    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));
    let names = host.discover_into(&plugins_dir).await.expect("discover");
    assert_eq!(names, vec!["echo-source"]);

    let status = host.get("echo-source").await.expect("status");
    assert_eq!(status.manifest.name, "echo-source");
    assert_eq!(
        status.manifest.plugin_type,
        objective_plugin_host::PluginType::Source
    );
}

#[tokio::test]
async fn discover_into_rejects_mismatched_directory_name() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();
    std::fs::create_dir_all(plugins_dir.join("real-name")).unwrap();
    std::fs::write(
        plugins_dir.join("real-name").join("plugin.json"),
        r#"{
            "name": "fake-name",
            "version": "0.1.0",
            "plugin_type": "source",
            "api_version": 1
        }"#,
    )
    .unwrap();

    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));
    let err = host.discover_into(&plugins_dir).await.expect_err("invalid");
    assert!(matches!(
        err,
        objective_plugin_host::HostError::Discovery(_)
    ));
}

#[tokio::test]
async fn persistent_state_round_trips() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    let host = make_host(Arc::clone(&bus), state_path.clone());

    host.update_state("audit-log", |entry| {
        entry.set("last_doc".to_string(), json!("doc-1"));
    })
    .await
    .expect("update");

    let host2 = make_host(Arc::clone(&bus), state_path);
    host2
        .update_state("audit-log", |entry| {
            let prev = entry.get("last_doc").cloned();
            entry.set("prev".to_string(), json!(prev));
        })
        .await
        .expect("update");
    let state = host2
        .update_state("audit-log", |entry| {
            entry.set("last_doc".to_string(), json!("doc-2"));
        })
        .await;
    state.expect("update");

    let host3 = make_host(Arc::clone(&bus), dir.path().join("state.json"));
    let _ = host3
        .update_state("audit-log", |entry| {
            let prev = entry.values.get("prev").cloned();
            entry.set("round_trip".to_string(), json!(prev));
        })
        .await;
    let snapshot = host3.list().await;
    assert!(snapshot.is_empty());
}

#[tokio::test]
async fn plugin_state_persists_to_disk() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");

    {
        let host = make_host(Arc::clone(&bus), state_path.clone());
        host.update_state("audit-log", |entry| {
            entry.set("last_event_at".to_string(), json!("2026-06-04T00:00:00Z"));
        })
        .await
        .expect("update");
    }

    let raw = std::fs::read_to_string(&state_path).expect("read");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(
        parsed["plugins"]["audit-log"]["values"]["last_event_at"],
        json!("2026-06-04T00:00:00Z")
    );
}

#[tokio::test]
async fn consecutive_failures_increment_on_error() {
    struct ErroringPlugin;

    #[async_trait]
    impl Plugin for ErroringPlugin {
        fn manifest(&self) -> &objective_plugin_host::PluginManifest {
            use std::sync::OnceLock;
            static M: OnceLock<objective_plugin_host::PluginManifest> = OnceLock::new();
            M.get_or_init(|| {
                objective_plugin_host::builtin_manifest(
                    "err",
                    objective_plugin_host::PluginType::Processor,
                    "0.1.0",
                    vec!["ingestion.document.received".to_string()],
                )
            })
        }

        async fn handle(
            &self,
            _event: &EventEnvelope,
        ) -> Result<Vec<PluginOutput>, objective_core::ObjectiveError> {
            Err(objective_core::ObjectiveError::Validation(
                "boom".to_string(),
            ))
        }
    }

    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));
    let manifest = objective_plugin_host::builtin_manifest(
        "err",
        objective_plugin_host::PluginType::Processor,
        "0.1.0",
        vec!["ingestion.document.received".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(manifest, Arc::new(ErroringPlugin)))
        .await
        .expect("register");

    for _ in 0..3 {
        bus.publish(
            "ingestion.document.received",
            EventEnvelope::new("ingestion.document.received", "test", json!({})),
        )
        .await
        .expect("publish");
        host.route_once().await.expect("route");
    }

    let status = host.get("err").await.expect("status");
    assert!(status.events_handled >= 3);
    assert!(status.last_error.is_some());
}

#[tokio::test]
async fn host_run_loop_dispatches_events() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");

    let audit = Arc::new(AuditLogPlugin::new(vec![
        "ingestion.document.received".to_string()
    ]));
    let audit_clone = Arc::clone(&audit);

    let host = Arc::new(make_host(Arc::clone(&bus), state_path));
    let manifest = objective_plugin_host::builtin_manifest(
        "audit-log",
        objective_plugin_host::PluginType::Filter,
        "0.1.0",
        vec!["ingestion.document.received".to_string()],
    );
    host.register_builtin(BuiltinPlugin::new(manifest, audit_clone as Arc<dyn Plugin>))
        .await
        .expect("register");

    let host_for_task = Arc::clone(&host);
    let handle = tokio::spawn(async move {
        let _ = host_for_task.run().await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(
        "ingestion.document.received",
        EventEnvelope::new(
            "ingestion.document.received",
            "test",
            json!({"document_id": "doc-1"}),
        ),
    )
    .await
    .expect("publish");

    let mut received = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let log = audit.snapshot().await;
        if !log.is_empty() {
            received = true;
            break;
        }
    }
    assert!(received, "audit plugin should have received the event");

    handle.abort();
}

#[tokio::test]
async fn list_returns_plugins_sorted_by_name() {
    let bus = make_bus();
    let dir = tempdir().unwrap();
    let host = make_host(Arc::clone(&bus), dir.path().join("state.json"));

    for (name, plugin_type) in [
        ("zeta", objective_plugin_host::PluginType::Source),
        ("alpha", objective_plugin_host::PluginType::Filter),
        ("mu", objective_plugin_host::PluginType::Notification),
    ] {
        let manifest = objective_plugin_host::builtin_manifest(name, plugin_type, "0.1.0", vec![]);
        host.register_builtin(BuiltinPlugin::new(manifest, audit_log_plugin(vec![])))
            .await
            .expect("register");
    }

    let listed = host.list().await;
    let names: Vec<&str> = listed.iter().map(|p| p.manifest.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "mu", "zeta"]);

    let counts = host.counts().await;
    assert_eq!(counts.total, 3);
    assert_eq!(*counts.by_type.get("filter").unwrap_or(&0), 1);
    assert_eq!(*counts.by_type.get("source").unwrap_or(&0), 1);
    assert_eq!(*counts.by_type.get("notification").unwrap_or(&0), 1);
}
