use super::types::*;

/// Build batch prompts for image generation.
/// Matches nanobanana buildBatchPrompts() - imageGenerator.ts:161-238
pub fn build_image_prompts(request: &ImageRequest) -> Vec<String> {
    let base_prompt = &request.prompt;
    let mut prompts: Vec<String> = Vec::new();

    // If no batch options, return original prompt
    if request.styles.is_empty() && request.variations.is_empty() && request.count <= 1 {
        return vec![base_prompt.clone()];
    }

    // Handle styles
    if !request.styles.is_empty() {
        for style in &request.styles {
            prompts.push(format!("{base_prompt}, {style} style"));
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
                match variation.as_str() {
                    "lighting" => {
                        variation_prompts.push(format!("{base_p}, dramatic lighting"));
                        variation_prompts.push(format!("{base_p}, soft lighting"));
                    }
                    "angle" => {
                        variation_prompts.push(format!("{base_p}, from above"));
                        variation_prompts.push(format!("{base_p}, close-up view"));
                    }
                    "color-palette" => {
                        variation_prompts.push(format!("{base_p}, warm color palette"));
                        variation_prompts.push(format!("{base_p}, cool color palette"));
                    }
                    "composition" => {
                        variation_prompts.push(format!("{base_p}, centered composition"));
                        variation_prompts
                            .push(format!("{base_p}, rule of thirds composition"));
                    }
                    "mood" => {
                        variation_prompts.push(format!("{base_p}, cheerful mood"));
                        variation_prompts.push(format!("{base_p}, dramatic mood"));
                    }
                    "season" => {
                        variation_prompts.push(format!("{base_p}, in spring"));
                        variation_prompts.push(format!("{base_p}, in winter"));
                    }
                    "time-of-day" => {
                        variation_prompts.push(format!("{base_p}, at sunrise"));
                        variation_prompts.push(format!("{base_p}, at sunset"));
                    }
                    _ => {}
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
pub fn build_icon_prompt(request: &IconRequest) -> String {
    let mut prompt = format!(
        "{}, {} style {}",
        request.prompt, request.style, request.icon_type
    );

    // Add corners only for app-icon type
    if request.icon_type == "app-icon" {
        prompt.push_str(&format!(", {} corners", request.corners));
    }

    // Add background only when not transparent
    if request.background != "transparent" {
        prompt.push_str(&format!(", {} background", request.background));
    }

    // Always append tail
    prompt.push_str(", clean design, high quality, professional");

    prompt
}

/// Build pattern prompt.
/// Matches nanobanana buildPatternPrompt() - index.ts:573-590
pub fn build_pattern_prompt(request: &PatternRequest) -> String {
    let mut prompt = format!(
        "{}, {} style {} pattern, {} density, {} colors",
        request.prompt, request.style, request.pattern_type, request.density, request.colors
    );

    // Add seamless suffix only for seamless type
    if request.pattern_type == "seamless" {
        prompt.push_str(", tileable, repeating pattern");
    }

    // Append tail with size
    prompt.push_str(&format!(", {} tile size, high quality", request.size));

    prompt
}

/// Build a single story step prompt.
/// Matches nanobanana generateStorySequence() prompt logic - imageGenerator.ts:418-441
pub fn build_story_step_prompt(request: &StoryRequest, step: u8, total_steps: u8) -> String {
    let mut prompt = format!(
        "{}, step {} of {}",
        request.prompt, step, total_steps
    );

    // Add context based on type
    match request.story_type.as_str() {
        "story" => {
            prompt.push_str(&format!(", narrative sequence, {} art style", request.style));
        }
        "process" => {
            prompt.push_str(", procedural step, instructional illustration");
        }
        "tutorial" => {
            prompt.push_str(", tutorial step, educational diagram");
        }
        "timeline" => {
            prompt.push_str(", chronological progression, timeline visualization");
        }
        _ => {}
    }

    // Add transition for steps after the first
    if step > 1 {
        prompt.push_str(&format!(
            ", {} transition from previous step",
            request.transition
        ));
    }

    prompt
}

/// Build logo prompt with optimized parameters for clean, text-free logo generation.
///
/// Requests a bright green (#00FF00) chromakey background instead of transparent,
/// because AI models cannot generate true transparency. The green background is
/// then removed in post-processing using HSV color space detection.
pub fn build_logo_prompt(request: &LogoRequest) -> String {
    format!(
        "{}, logo design, icon only, no text, no words, no letters, {} style, solid bright green (#00FF00) chroma key background, thin white outline around the logo, logo fills most of the canvas with minimal margins, large prominent design, clean professional, high contrast, 2-3 colors maximum, simple shapes",
        request.prompt, request.style
    )
}

/// Build diagram prompt.
/// Matches nanobanana buildDiagramPrompt() - index.ts:592-607
pub fn build_diagram_prompt(request: &DiagramRequest) -> String {
    let mut prompt = format!(
        "{}, {} diagram, {} style, {} layout",
        request.prompt, request.diagram_type, request.style, request.layout
    );

    prompt.push_str(&format!(
        ", {} level of detail, {} color scheme",
        request.complexity, request.colors
    ));

    prompt.push_str(&format!(
        ", {} annotations and labels",
        request.annotations
    ));

    prompt.push_str(", clean technical illustration, clear visual hierarchy");

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_image_prompts_no_options() {
        let request = ImageRequest {
            prompt: "a cat in space".into(),
            count: 1,
            styles: vec![],
            variations: vec![],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&request);
        assert_eq!(prompts, vec!["a cat in space"]);
    }

    #[test]
    fn test_build_image_prompts_with_styles() {
        let request = ImageRequest {
            prompt: "sunset".into(),
            count: 4,
            styles: vec!["watercolor".into(), "anime".into()],
            variations: vec![],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&request);
        assert_eq!(
            prompts,
            vec!["sunset, watercolor style", "sunset, anime style"]
        );
    }

    #[test]
    fn test_build_image_prompts_with_variations() {
        let request = ImageRequest {
            prompt: "mountain".into(),
            count: 8,
            styles: vec![],
            variations: vec!["lighting".into()],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&request);
        assert_eq!(
            prompts,
            vec!["mountain, dramatic lighting", "mountain, soft lighting"]
        );
    }

    #[test]
    fn test_build_image_prompts_with_styles_and_variations() {
        let request = ImageRequest {
            prompt: "sunset".into(),
            count: 8,
            styles: vec!["watercolor".into()],
            variations: vec!["mood".into()],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&request);
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
        let request = ImageRequest {
            prompt: "test".into(),
            count: 1,
            styles: vec!["a".into(), "b".into()],
            variations: vec![],
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompts = build_image_prompts(&request);
        assert_eq!(prompts.len(), 1);
    }

    #[test]
    fn test_build_icon_prompt() {
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
        let prompt = build_icon_prompt(&request);
        assert_eq!(
            prompt,
            "rocket logo, modern style app-icon, rounded corners, clean design, high quality, professional"
        );
    }

    #[test]
    fn test_build_icon_prompt_with_background() {
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
        let prompt = build_icon_prompt(&request);
        assert_eq!(
            prompt,
            "star, flat style favicon, white background, clean design, high quality, professional"
        );
    }

    #[test]
    fn test_build_pattern_prompt_seamless() {
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
        let prompt = build_pattern_prompt(&request);
        assert_eq!(
            prompt,
            "geometric tiles, abstract style seamless pattern, medium density, colorful colors, tileable, repeating pattern, 256x256 tile size, high quality"
        );
    }

    #[test]
    fn test_build_pattern_prompt_texture() {
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
        let prompt = build_pattern_prompt(&request);
        assert!(!prompt.contains("tileable"));
        assert!(prompt.contains("wood grain, organic style texture pattern"));
    }

    #[test]
    fn test_build_story_step_prompt() {
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

        let step1 = build_story_step_prompt(&request, 1, 4);
        assert_eq!(
            step1,
            "making coffee, step 1 of 4, procedural step, instructional illustration"
        );

        let step2 = build_story_step_prompt(&request, 2, 4);
        assert!(step2.contains("smooth transition from previous step"));
    }

    #[test]
    fn test_build_diagram_prompt() {
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
        let prompt = build_diagram_prompt(&request);
        assert_eq!(
            prompt,
            "user auth flow, flowchart diagram, professional style, hierarchical layout, detailed level of detail, accent color scheme, detailed annotations and labels, clean technical illustration, clear visual hierarchy"
        );
    }

    #[test]
    fn test_build_logo_prompt() {
        let request = LogoRequest {
            prompt: "rocket".into(),
            style: "flat".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_logo_prompt(&request);
        assert!(prompt.starts_with("rocket, logo design"));
        assert!(prompt.contains("no text"));
        assert!(prompt.contains("no words"));
        assert!(prompt.contains("no letters"));
        assert!(prompt.contains("flat style"));
        assert!(prompt.contains("green"));
        assert!(prompt.contains("chroma key"));
        assert!(prompt.contains("white outline"));
        assert!(prompt.contains("fills most of the canvas"));
        assert!(prompt.contains("large prominent"));
        assert!(prompt.contains("simple shapes"));
    }

    #[test]
    fn test_build_logo_prompt_minimal_style() {
        let request = LogoRequest {
            prompt: "mountain".into(),
            style: "minimal".into(),
            format: OutputFormat::Png,
            output_dir: None,
            dry_run: false,
        };
        let prompt = build_logo_prompt(&request);
        assert!(prompt.contains("minimal style"));
        assert!(prompt.contains("icon only"));
    }
}
