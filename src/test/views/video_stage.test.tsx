import { VideoStage, type VideoStageStatus } from "../../components/VideoStage";

export const videoStageChecks = {
  component: Boolean(VideoStage),
  fallbackStatuses: ["not-checked", "unavailable", "error", "fallback"] satisfies VideoStageStatus[],
  lifecycleStatuses: ["loading", "ready", "playing", "paused", "ended"].map((status) => status satisfies VideoStageStatus),
  usesHtmlVideo: true,
  fullscreenLabel: "Enter fullscreen",
  clockLifecycle: ["onResize", "onSeek", "onPause", "onResume", "onEnded"],
};

if (!videoStageChecks.component || !videoStageChecks.usesHtmlVideo) throw new Error("Video stage contract failed");
