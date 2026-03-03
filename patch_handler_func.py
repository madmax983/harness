import re

with open("crates/harness-mcp/src/handler.rs", "r") as f:
    content = f.read()

# Add handler function
pattern = r'(confidence: assessment\.confidence,\n\s*\})\n\s*\}'
replacement = r'''confidence: assessment.confidence,
        })
    }

    async fn handle_distill_trajectories(
        &self,
        req: tools::DistillTrajectoriesRequest,
    ) -> HandlerResult<tools::DistillTrajectoriesResponse> {
        use harness_sona::experimental::distiller::{ChatFormatDistiller, TrajectoryDistiller};

        let recorder = self.state.trajectory_recorder();
        let limit = req.limit.unwrap_or(100);

        let events = recorder
            .query(harness_persistence::TrajectoryQuery::new().with_limit(limit))
            .await
            .unwrap_or_default();

        let distiller = ChatFormatDistiller::default();
        let mut output = Vec::new();

        distiller
            .distill(&events, &mut output)
            .map_err(|e| HandlerError::InternalError(format!("Distillation failed: {e}")))?;

        let dataset = String::from_utf8_lossy(&output).into_owned();
        let distilled_count = dataset.lines().count();

        Ok(tools::DistillTrajectoriesResponse {
            dataset,
            distilled_count,
        })
    }
}'''

content = re.sub(pattern, replacement, content)

with open("crates/harness-mcp/src/handler.rs", "w") as f:
    f.write(content)
