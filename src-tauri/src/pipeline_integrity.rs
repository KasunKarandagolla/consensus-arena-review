//! Executable cross-mechanism Pipeline Integrity fixtures.
//!
//! These tests deliberately exercise controller contracts across route, gate,
//! resource, experiment, consultation, and recovery boundaries without live
//! providers. They complement module-local tests; they are not a substitute
//! for runtime qualification.

#[cfg(test)]
mod tests {
    use crate::consultation_broker::{
        ConsultationProvider, ConsultationReason, ConsultationTransactionState,
        ConsultationTransportKind, ConsultationWorkOrder, ConversationAnchor,
        ConversationAnchorUpdate, ConversationAvailability, ConversationEstablishment,
        apply_anchor_update, validate_application_url,
    };
    use crate::evidence_gates::{GateId, GateStatus};
    use crate::pipeline_contract::{
        ArchitecturePlanningMode, ArchitectureProposalInput, ArchitectureReviewPacket,
        ExperimentContract, ExperimentExecutorKind, ExperimentOperation, GateRemediationOutcome,
        OwnerDecisionKind, ProductRoute, ResourceClass, ResourceScheduler,
        architecture_planning_mode, owner_decision_for_option, route_gate_remediation, route_plan,
        select_route, select_route_with_context,
    };

    fn now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    fn consultation_order() -> ConsultationWorkOrder {
        ConsultationWorkOrder::new(
            "project".to_string(),
            "run".to_string(),
            "decision".to_string(),
            ConsultationReason::ArchitectureConflict,
            ConsultationProvider::ChatGpt,
            "chatgpt-consumer".to_string(),
            "arena-chatgpt".to_string(),
            ConsultationTransportKind::ExternalBrowserAgent,
            1,
            1,
            "question",
            "bounded disclosure",
            now() + 600,
            1,
        )
        .expect("fixture consultation")
    }

    #[test]
    fn scenario_a_strong_greenfield_uses_full_discover_decide_deliver_release_path() {
        let route = select_route(
            "Build a new desktop product that helps small audit teams reconcile review evidence",
        );
        assert_eq!(route, ProductRoute::NewProduct);
        let plan = route_plan(route);
        assert_eq!(
            plan.stages,
            vec![
                crate::pipeline_contract::PipelineStage::Discover,
                crate::pipeline_contract::PipelineStage::Decide,
                crate::pipeline_contract::PipelineStage::Deliver,
                crate::pipeline_contract::PipelineStage::Release,
            ]
        );
        assert!(plan.omitted_stages.is_empty());
        assert_eq!(
            architecture_planning_mode(route, "new product with uncertain architecture"),
            ArchitecturePlanningMode::CompetingProposals
        );
    }

    #[test]
    fn scenario_a_greenfield_intent_survives_nonempty_scaffold_context() {
        assert_eq!(
            select_route_with_context(
                "Build a new desktop product from scratch for audit evidence review",
                true,
            ),
            ProductRoute::NewProduct
        );
    }

    #[test]
    fn scenario_b_weak_greenfield_cannot_loop_research_forever() {
        let route = select_route(
            "Build a new social app despite weak evidence and many established alternatives",
        );
        assert_eq!(route, ProductRoute::NewProduct);
        let first = route_gate_remediation(GateId::ProblemResearch, GateStatus::MissingEvidence, 0);
        assert_eq!(first.outcome, GateRemediationOutcome::NeedsResearch);
        let exhausted =
            route_gate_remediation(GateId::ProblemResearch, GateStatus::MissingEvidence, 2);
        assert_eq!(exhausted.outcome, GateRemediationOutcome::RecommendPivot);
        assert_eq!(
            owner_decision_for_option(None, "pivot"),
            Ok(OwnerDecisionKind::PivotRun)
        );
    }

    #[test]
    fn scenario_c_technical_uncertainty_requires_exact_typed_experiment() {
        let contract = ExperimentContract {
            experiment_id: "probe-library-capability".to_string(),
            synthesis_identity: "synthesis-1".to_string(),
            assumption: "the repository exposes the required marker".to_string(),
            executor_kind: ExperimentExecutorKind::ToolProbe,
            operation: ExperimentOperation::FileContains {
                relative_path: "src/lib.rs".to_string(),
                needle: "required_boundary".to_string(),
            },
            expected_observation: "marker exists".to_string(),
            pass_condition: "marker exists".to_string(),
            fail_condition: "marker is absent".to_string(),
            inconclusive_condition: "file cannot be read".to_string(),
            environment: "isolated experiment worktree".to_string(),
            timeout_seconds: 60,
            allowed_effects: vec!["read-only isolated probe".to_string()],
            protected_paths: vec![".arena/verification.json".to_string()],
        };
        assert!(contract.validate().is_ok());

        let mut mismatched = contract;
        mismatched.executor_kind = ExperimentExecutorKind::DeterministicCommand;
        assert!(mismatched.validate().is_err());
    }

    #[test]
    fn scenario_d_existing_feature_skips_market_discovery_and_can_use_established_pattern() {
        let idea = "Add CSV export to this existing repo using the existing utility and established pattern";
        let route = select_route(idea);
        assert_eq!(route, ProductRoute::ExistingFeature);
        let plan = route_plan(route);
        assert!(
            !plan
                .stages
                .contains(&crate::pipeline_contract::PipelineStage::Discover)
        );
        assert!(
            plan.omitted_stages
                .iter()
                .any(|stage| stage.stage == crate::pipeline_contract::PipelineStage::Discover)
        );
        assert_eq!(
            architecture_planning_mode(route, idea),
            ArchitecturePlanningMode::EstablishedPattern
        );
    }

    #[test]
    fn scenario_e_incident_begins_with_reproduction_not_greenfield_research() {
        let route = select_route(
            "Production failure in this existing app: invoice import crashes after the latest change",
        );
        assert_eq!(route, ProductRoute::Incident);
        let plan = route_plan(route);
        assert_eq!(
            plan.stages.first(),
            Some(&crate::pipeline_contract::PipelineStage::ReproduceDiagnose)
        );
        assert!(
            !plan
                .stages
                .contains(&crate::pipeline_contract::PipelineStage::Discover)
        );
    }

    #[test]
    fn fault_resource_saturation_rejects_second_exclusive_owner() {
        let scheduler = ResourceScheduler::default();
        let first = scheduler
            .try_claim("research-a", 1, ResourceClass::ExclusiveSessionRuntime)
            .expect("first owner");
        assert!(
            scheduler
                .try_claim("architect-b", 1, ResourceClass::ExclusiveSessionRuntime)
                .is_err()
        );
        scheduler.release(&first).expect("exact owner releases");
    }

    #[test]
    fn fault_missing_architecture_artifact_is_rejected_before_review() {
        let a = ArchitectureProposalInput {
            evidence_id: "a".to_string(),
            proposal: "proposal a".to_string(),
            assumptions: vec!["assumption".to_string()],
            reuse_choices: vec!["reuse".to_string()],
            interfaces: vec!["boundary".to_string()],
            risks: vec!["risk".to_string()],
            content: "proposal a".to_string(),
        };
        let missing = ArchitectureProposalInput {
            evidence_id: "b".to_string(),
            proposal: String::new(),
            assumptions: Vec::new(),
            reuse_choices: Vec::new(),
            interfaces: Vec::new(),
            risks: Vec::new(),
            content: String::new(),
        };
        assert!(ArchitectureReviewPacket::resolve("project".to_string(), 1, a, missing).is_err());
    }

    #[test]
    fn fault_unknown_consultation_outcome_has_no_transition_back_to_send_authority() {
        let mut order = consultation_order();
        order
            .transition(ConsultationTransactionState::Staged)
            .expect("stage");
        order
            .transition(ConsultationTransactionState::Armed)
            .expect("arm");
        order
            .transition(ConsultationTransactionState::UnknownOutcome)
            .expect("unknown");
        assert!(
            order
                .transition(ConsultationTransactionState::Armed)
                .is_err()
        );
        assert!(
            order
                .transition(ConsultationTransactionState::Staged)
                .is_err()
        );
        assert!(
            order
                .transition(ConsultationTransactionState::Observing)
                .is_ok()
        );
    }

    #[test]
    fn fault_anchor_origin_and_revision_drift_are_rejected() {
        assert!(
            validate_application_url(
                ConsultationProvider::ChatGpt,
                "https://chatgpt.com.evil.example/c/123",
                true
            )
            .is_err()
        );

        let order = consultation_order();
        let current = ConversationAnchor {
            establishment: ConversationEstablishment::Established,
            availability: ConversationAvailability::Available,
            canonical_url: Some("https://chatgpt.com/c/123".to_string()),
            provider_conversation_id: Some("123".to_string()),
            revision: 4,
            ..ConversationAnchor::initial(&order, "adapter-v1")
        };
        let stale = ConversationAnchorUpdate {
            expected_revision: 3,
            availability: ConversationAvailability::Available,
            established: true,
            canonical_url: Some("https://chatgpt.com/c/123".to_string()),
            provider_conversation_id: Some("123".to_string()),
            provider_branch_id: None,
            pending_request_id: None,
            last_confirmed_user_turn_digest: Some("user".to_string()),
            last_confirmed_assistant_turn_digest: Some("assistant".to_string()),
            adapter_version: "adapter-v1".to_string(),
        };
        assert!(apply_anchor_update(&current, stale).is_err());
    }

    #[test]
    fn fault_owner_authority_options_are_exact_not_semantic_prose() {
        assert_eq!(
            owner_decision_for_option(
                Some(crate::evidence_gates::DecisionOutcome::ValidationExperiment),
                "authorize_validation_experiment"
            ),
            Ok(OwnerDecisionKind::AuthorizeValidationExperiment)
        );
        assert!(
            owner_decision_for_option(
                Some(crate::evidence_gates::DecisionOutcome::ValidationExperiment),
                "yes go ahead"
            )
            .is_err()
        );
        assert!(
            owner_decision_for_option(
                Some(crate::evidence_gates::DecisionOutcome::NarrowBuild),
                "authorize_validation_experiment"
            )
            .is_err()
        );
    }

    #[test]
    fn fault_apply_authority_requires_the_exact_owner_action() {
        assert_eq!(
            owner_decision_for_option(None, "approve_apply"),
            Ok(OwnerDecisionKind::ApproveApply)
        );
        assert!(owner_decision_for_option(None, "apply it").is_err());
        assert!(owner_decision_for_option(None, "authorize_narrow_build").is_err());
    }

    #[test]
    fn fault_gate_failure_always_has_bounded_route_or_explicit_blocker() {
        for gate in [
            GateId::Vision,
            GateId::ProblemResearch,
            GateId::Positioning,
            GateId::Ambiguity,
            GateId::Reuse,
            GateId::Architecture,
            GateId::BuildReadiness,
            GateId::Implementation,
            GateId::Release,
        ] {
            let remediation = route_gate_remediation(gate, GateStatus::Blocked, 0);
            assert_ne!(remediation.outcome, GateRemediationOutcome::Satisfied);
            assert!(!remediation.action.trim().is_empty());
            assert!(remediation.attempt <= remediation.max_attempts);
        }
    }
}
