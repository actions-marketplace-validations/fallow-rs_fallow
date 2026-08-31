import cors from "cors";

type CorsOptions = {
  origin: string;
  credentials: boolean;
};

export const middleware = cors({
  origin: "*",
  credentials: true,
} satisfies CorsOptions);
