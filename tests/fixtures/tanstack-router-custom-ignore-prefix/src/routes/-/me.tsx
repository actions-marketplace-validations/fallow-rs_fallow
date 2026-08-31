import { createFileRoute } from "@tanstack/react-router";
import { MeProfile } from "../../components/Me";
import { loadProfile } from "../../services/profile";

export const Route = createFileRoute("/-/me")({
  loader: () => loadProfile(),
  component: MeProfile,
});

export const unusedMeHelper = 1;
