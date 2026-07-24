import { createRemoteJWKSet, jwtVerify } from "jose";
import { headers } from "next/headers";
import { redirect } from "next/navigation";

export type ChatGPTUser = {
  displayName: string;
  email: string;
  fullName: string | null;
};

const USER_EMAIL_HEADER = "oai-authenticated-user-email";
const USER_FULL_NAME_HEADER = "oai-authenticated-user-full-name";
const USER_FULL_NAME_ENCODING_HEADER =
  "oai-authenticated-user-full-name-encoding";
const ACCESS_JWT_HEADER = "cf-access-jwt-assertion";
const PERCENT_ENCODED_UTF8 = "percent-encoded-utf-8";
const SIGN_IN_PATH = "/signin-with-chatgpt";
const SIGN_OUT_PATH = "/signout-with-chatgpt";
const CALLBACK_PATH = "/callback";

export async function getChatGPTUser(): Promise<ChatGPTUser | null> {
  const requestHeaders = await headers();
  const teamDomain = process.env.TEAM_DOMAIN?.replace(/\/+$/, "");
  const policyAudience = process.env.POLICY_AUD;

  if (teamDomain || policyAudience) {
    if (!teamDomain || !policyAudience) {
      console.error(JSON.stringify({
        message: "Cloudflare Access JWT configuration is incomplete",
      }));
      return null;
    }

    const token = requestHeaders.get(ACCESS_JWT_HEADER);
    if (!token) return null;

    try {
      const keySet = createRemoteJWKSet(
        new URL(`${teamDomain}/cdn-cgi/access/certs`),
      );
      const { payload } = await jwtVerify(token, keySet, {
        issuer: teamDomain,
        audience: policyAudience,
      });
      const email = typeof payload.email === "string"
        ? payload.email.trim().toLowerCase()
        : "";
      if (!email) return null;

      const fullName = typeof payload.name === "string"
        ? payload.name.trim() || null
        : null;
      return {
        displayName: fullName ?? email,
        email,
        fullName,
      };
    } catch (error) {
      console.warn(JSON.stringify({
        message: "Cloudflare Access JWT verification failed",
        error: error instanceof Error ? error.message : "Unknown error",
      }));
      return null;
    }
  }

  const email = requestHeaders.get(USER_EMAIL_HEADER);
  if (!email) return null;

  const encodedFullName = requestHeaders.get(USER_FULL_NAME_HEADER);
  const fullName =
    encodedFullName &&
    requestHeaders.get(USER_FULL_NAME_ENCODING_HEADER) === PERCENT_ENCODED_UTF8
      ? safeDecodeURIComponent(encodedFullName)
      : null;

  return {
    displayName: fullName ?? email,
    email,
    fullName,
  };
}

export async function requireChatGPTUser(
  returnTo: string,
): Promise<ChatGPTUser> {
  const user = await getChatGPTUser();
  if (user) return user;

  redirect(chatGPTSignInPath(returnTo));
}

export function chatGPTSignInPath(returnTo: string): string {
  const safeReturnTo = safeRelativeReturnPath(returnTo);
  return `${SIGN_IN_PATH}?return_to=${encodeURIComponent(safeReturnTo)}`;
}

export function chatGPTSignOutPath(returnTo = "/"): string {
  const safeReturnTo = safeRelativeReturnPath(returnTo);
  return `${SIGN_OUT_PATH}?return_to=${encodeURIComponent(safeReturnTo)}`;
}

function safeRelativeReturnPath(value: string): string {
  if (!value.startsWith("/") || value.startsWith("//")) return "/";

  let url: URL;
  try {
    url = new URL(value, "https://app.local");
  } catch {
    return "/";
  }
  if (url.origin !== "https://app.local") return "/";
  if (isReservedAuthPath(url.pathname)) return "/";

  return `${url.pathname}${url.search}${url.hash}`;
}

function isReservedAuthPath(pathname: string): boolean {
  return (
    pathname === SIGN_IN_PATH ||
    pathname === SIGN_OUT_PATH ||
    pathname === CALLBACK_PATH
  );
}

function safeDecodeURIComponent(value: string): string | null {
  try {
    return decodeURIComponent(value);
  } catch {
    return null;
  }
}
