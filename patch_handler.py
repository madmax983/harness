import re

with open("crates/harness-mcp/src/handler.rs", "r") as f:
    content = f.read()

# Add distill_trajectories handler call
pattern = r'("check_safety" => \{\n\s*let req: tools::CheckSafetyRequest = serde_json::from_value\(arguments\)\n\s*\.map_err\(\|e\| HandlerError::InvalidArgs\(e\.to_string\(\)\)\)\?;\n\s*let resp = self\.handle_check_safety\(req\)\.await\?;\n\s*Ok\(serde_json::to_value\(resp\)\.unwrap\(\)\)\n\s*\})'

replacement = r'''\1
            "distill_trajectories" => {
                let req: tools::DistillTrajectoriesRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_distill_trajectories(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }'''

content = re.sub(pattern, replacement, content)

# Add distill_trajectories to tool names
pattern2 = r'("check_safety",\n\s*\])'
replacement2 = r'"check_safety",\n            "distill_trajectories",\n        ]'
content = re.sub(pattern2, replacement2, content)

# Add handler function
pattern3 = r'(confidence: assessment\.confidence,\n\s*\})\n\s*\}'
replacement3 = r'''confidence: assessment.confidence,
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

content = re.sub(pattern3, replacement3, content)

# Update test assertions
pattern4 = r'(assert!\(names\.contains\(&"check_safety"\)\);\n\s*assert_eq!\(names\.len\(\), 74\);)'
replacement4 = r'assert!(names.contains(&"check_safety"));\n        assert!(names.contains(&"distill_trajectories"));\n        assert_eq!(names.len(), 75);'
content = re.sub(pattern4, replacement4, content)

with open("crates/harness-mcp/src/handler.rs", "w") as f:
    f.write(content)
