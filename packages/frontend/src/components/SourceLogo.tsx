"use client";

interface SourceLogoProps {
  sourceId: string;
  height?: number;
  className?: string;
}

export function SourceLogo({ sourceId, height = 14, className = "" }: SourceLogoProps) {
  const normalizedId = sourceId.toLowerCase();

  const getLogoSrc = (id: string) => {
    switch (id) {
      case "opencode":
        return "/assets/logos/opencode.png";
      case "claude":
        return "/assets/logos/claude.jpg";
      case "codex":
        return "/assets/logos/openai.jpg";
      case "copilot":
        return "https://raw.githubusercontent.com/junhoyeo/tokscale/main/.github/assets/client-copilot.jpg";
      case "gemini":
        return "/assets/logos/gemini.png";
      case "cursor":
        return "/assets/logos/cursor.jpg";
      case "amp":
        return "/assets/logos/amp.png";
      case "droid":
        return "/assets/logos/droid.png";
      case "openclaw":
        return "/assets/logos/openclaw.jpg";
      case "hermes":
        return "/assets/logos/hermes.png";
      case "pi":
        return "/assets/logos/pi.png";
      case "kimi":
        return "/assets/logos/kimi.png";
      case "qwen":
        return "/assets/logos/qwen.png";
      case "roocode":
        return "/assets/logos/roocode.png";
      case "kilocode":
      case "kilo":
        return "/assets/logos/kilocode.png";
      case "mux":
        return "/assets/logos/mux.png";
      case "crush":
        return "https://raw.githubusercontent.com/junhoyeo/tokscale/main/.github/assets/client-crush.png";
      case "kiro":
        return "/assets/logos/kiro.ico";
      case "zed":
        return "https://raw.githubusercontent.com/junhoyeo/tokscale/main/.github/assets/client-zed.webp";
      case "cline":
        return "/assets/logos/cline.png";
      case "synthetic":
        return "/assets/logos/synthetic.png";
      default:
        return null;
    }
  };

  const src = getLogoSrc(normalizedId);

  if (!src) {
    return <span className={className}>{sourceId}</span>;
  }

  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={src}
      alt={sourceId}
      className={`rounded-sm object-contain ${className}`}
      style={{ height, width: "auto", minWidth: height, maxWidth: height, minHeight: height, maxHeight: height }}
    />
  );
}
