use super::config::*;
use super::types::*;

/// Build batch prompts for image generation.
/// Matches nanobanana buildBatchPrompts() - imageGenerator.ts:161-238
pub fn build_image_prompts(config: &ImageConfig, request: &ImageRequest) -> Vec<String> {
    let base_prompt = &request.prompt;
    let mut prompts: Vec<String> = Vec::new();

    // If no batch options, return original prompt
    if request.styles.is_empty() && request.variations.is_empty() && request.count <= 1 {
        return vec![base_prompt.clone()];
    }

    // Handle styles
    if !request.styles.is_empty() {
        for style in &request.styles {
            let suffix = config.prompt.style_suffix.replace("{style}", style);
            prompts.push(format!("{base_prompt}{suffix}"));
        }
    }

    // Handle variations
    if !request.variations.is_empty() {
        let base_prompts = if prompts.is_empty() {
            vec![base_prompt.clone()]
        } else {
            prompts.clone()
        };
        let mut variation_prompts: Vec<String> = Vec::new();

        for base_p in &base_prompts {
            for variation in &request.variations {
                if let Some(suffixes) = config.variations.get(variation) {
                    for suffix in suffixes {
                        variation_prompts.push(format!("{base_p}{suffix}"));
                    }
                }
            }
        }

        if !variation_prompts.is_empty() {
            prompts = variation_prompts;
        }
    }

    // If no styles/variations but count > 1, create simple variations
    if prompts.is_empty() && request.count > 1 {
        for _ in 0..request.count {
            prompts.push(base_prompt.clone());
        }
    }

    // Limit to count if specified
    if request.count > 0 && prompts.len() > request.count as usize {
        prompts.truncate(request.count as usize);
    }

    if prompts.is_empty() {
        vec![base_prompt.clone()]
    } else {
        prompts
    }
}

/// Build icon prompt.
/// Matches nanobanana buildIconPrompt() - index.ts:551-571
pub fn build_icon_prompt(config: &IconConfig, request: &IconRequest) -> String {
    let mut prompt = config
        .prompt
        .base
        .replace("{prompt}", &request.prompt)
        .replace("{style}", &request.style)
        .replace("{type}", &request.icon_type);

    // Add corners only for app-icon type
    if request.icon_type == "app-icon" {
        prompt.push_str(
            &config
                .prompt
                .app_icon_suffix
                .replace("{corners}", &request.corners),
        );
    }

    // Add background only when not transparent
    if request.background != "transparent" {
        prompt.push_str(
            &config
                .prompt
                .background_suffix
                .replace("{background}", &request.background),
        );
    }

    // Always append tail
    prompt.push_str(&config.prompt.tail);

    prompt
}

/// Build pattern prompt.
/// Matches nanobanana buildPatternPrompt() - index.ts:573-590
pub fn build_pattern_prompt(config: &PatternConfig, request: &PatternRequest) -> String {
    let mut prompt = config
        .prompt
        .base
        .replace("{prompt}", &request.prompt)
        .replace("{style}", &request.style)
        .replace("{type}", &request.pattern_type)
        .replace("{density}", &request.density)
        .replace("{colors}", &request.colors);

    // Add seamless suffix only for seamless type
    if request.pattern_type == "seamless" {
        prompt.push_str(&config.prompt.seamless_suffix);
    }

    // Append tail with size
    prompt.push_str(&config.prompt.tail.replace("{size}", &request.size));

    prompt
}

/// Build a single story step prompt.
/// Matches nanobanana generateStorySequence() prompt logic - imageGenerator.ts:418-441
pub fn build_story_step_prompt(
    config: &StoryConfig,
    request: &StoryRequest,
    step: u8,
    total_steps: u8,
) -> String {
    let mut prompt = config
        .prompt
        .base
        .replace("{prompt}", &request.prompt)
        .replace("{step}", &step.to_string())
        .replace("{total_steps}", &total_steps.to_string());

    // Add type context
    if let Some(context) = config.type_context.get(&request.story_type) {
        prompt.push_str(&context.replace("{style}", &request.style));
    }

    // Add transition for steps after the first
    if step > 1 {
        prompt.push_str(
            &config
                .transitions
                .template
                .replace("{transition}", &request.transition),
        );
    }

    prompt
}

/// Build diagram prompt.
/// Matches nanobanana buildDiagramPrompt() - index.ts:592-607
pub fn build_diagram_prompt(config: &DiagramConfig, request: &DiagramRequest) -> String {
    let mut prompt = config
        .prompt
        .base
        .replace("{prompt}", &request.prompt)
        .replace("{type}", &request.diagram_type)
        .replace("{style}", &request.style)
        .replace("{layout}", &request.layout);

    prompt.push_str(
        &config
            .prompt
            .detail
            .replace("{complexity}", &request.complexity)
            .replace("{colors}", &request.colors),
    );

    prompt.push_str(
        &config
            .prompt
            .annotation
            .replace("{annotations}", &request.annotations),
    );

    prompt.push_str(&config.prompt.tail);

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::config;

    #[test]
    fn test_build_image_prompts_no_options() {
        let cfg = config::load_image_config().unwrap();
        let request = ImageRequest {
            prompt: "a cat in space".into(),
            count: 1,
            styles: vec![],
            variations: vec![],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&cfg, &request);
        assert_eq!(prompts, vec!["a cat in space"]);
    }

    #[test]
    fn test_build_image_prompts_with_styles() {
        let cfg = config::load_image_config().unwrap();
        let request = ImageRequest {
            prompt: "sunset".into(),
            count: 4,
            styles: vec!["watercolor".into(), "anime".into()],
            variations: vec![],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&cfg, &request);
        assert_eq!(
            prompts,
            vec!["sunset, watercolor style", "sunset, anime style"]
        );
    }

    #[test]
    fn test_build_image_prompts_with_variations() {
        let cfg = config::load_image_config().unwrap();
        let request = ImageRequest {
            prompt: "mountain".into(),
            count: 8,
            styles: vec![],
            variations: vec!["lighting".into()],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&cfg, &request);
        assert_eq!(
            prompts,
            vec![
                "mountain, dramatic lighting",
                "mountain, soft lighting"
            ]
        );
    }

    #[test]
    fn test_build_image_prompts_with_styles_and_variations() {
        let cfg = config::load_image_config().unwrap();
        let request = ImageRequest {
            prompt: "sunset".into(),
            count: 8,
            styles: vec!["watercolor".into()],
            variations: vec!["mood".into()],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&cfg, &request);
        assert_eq!(
            prompts,
            vec![
                "sunset, watercolor style, cheerful mood",
                "sunset, watercolor style, dramatic mood"
            ]
        );
    }

    #[test]
    fn test_build_image_prompts_count_limit() {
        let cfg = config::load_image_config().unwrap();
        let request = ImageRequest {
            prompt: "test".into(),
            count: 1,
            styles: vec!["a".into(), "b".into()],
            variations: vec![],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&cfg, &request);
        assert_eq!(prompts.len(), 1);
    }

    #[test]
    fn test_build_icon_prompt() {
        let cfg = config::load_icon_config().unwrap();
        let request = IconRequest {
            prompt: "rocket logo".into(),
            sizes: vec![256],
            icon_type: "app-icon".into(),
            style: "modern".into(),
            background: "transparent".into(),
            corners: "rounded".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_icon_prompt(&cfg, &request);
        assert_eq!(
            prompt,
            "rocket logo, modern style app-icon, rounded corners, clean design, high quality, professional"
        );
    }

    #[test]
    fn test_build_icon_prompt_with_background() {
        let cfg = config::load_icon_config().unwrap();
        let request = IconRequest {
            prompt: "star".into(),
            sizes: vec![64],
            icon_type: "favicon".into(),
            style: "flat".into(),
            background: "white".into(),
            corners: "sharp".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_icon_prompt(&cfg, &request);
        assert_eq!(
            prompt,
            "star, flat style favicon, white background, clean design, high quality, professional"
        );
    }

    #[test]
    fn test_build_pattern_prompt_seamless() {
        let cfg = config::load_pattern_config().unwrap();
        let request = PatternRequest {
            prompt: "geometric tiles".into(),
            pattern_type: "seamless".into(),
            style: "abstract".into(),
            density: "medium".into(),
            colors: "colorful".into(),
            size: "256x256".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_pattern_prompt(&cfg, &request);
        assert_eq!(
            prompt,
            "geometric tiles, abstract style seamless pattern, medium density, colorful colors, tileable, repeating pattern, 256x256 tile size, high quality"
        );
    }

    #[test]
    fn test_build_pattern_prompt_texture() {
        let cfg = config::load_pattern_config().unwrap();
        let request = PatternRequest {
            prompt: "wood grain".into(),
            pattern_type: "texture".into(),
            style: "organic".into(),
            density: "dense".into(),
            colors: "mono".into(),
            size: "512x512".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_pattern_prompt(&cfg, &request);
        // no seamless_suffix for texture type
        assert!(!prompt.contains("tileable"));
        assert!(prompt.contains("wood grain, organic style texture pattern"));
    }

    #[test]
    fn test_build_story_step_prompt() {
        let cfg = config::load_story_config().unwrap();
        let request = StoryRequest {
            prompt: "making coffee".into(),
            steps: 4,
            story_type: "process".into(),
            style: "consistent".into(),
            transition: "smooth".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };

        let step1 = build_story_step_prompt(&cfg, &request, 1, 4);
        assert_eq!(
            step1,
            "making coffee, step 1 of 4, procedural step, instructional illustration"
        );

        let step2 = build_story_step_prompt(&cfg, &request, 2, 4);
        assert!(step2.contains("smooth transition from previous step"));
    }

    #[test]
    fn test_build_diagram_prompt() {
        let cfg = config::load_diagram_config().unwrap();
        let request = DiagramRequest {
            prompt: "user auth flow".into(),
            diagram_type: "flowchart".into(),
            style: "professional".into(),
            layout: "hierarchical".into(),
            complexity: "detailed".into(),
            colors: "accent".into(),
            annotations: "detailed".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_diagram_prompt(&cfg, &request);
        assert_eq!(
            prompt,
            "user auth flow, flowchart diagram, professional style, hierarchical layout, detailed level of detail, accent color scheme, detailed annotations and labels, clean technical illustration, clear visual hierarchy"
        );
    }
}
