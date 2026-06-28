// Anonymous User Identity System
// Combines localStorage-based anonymous UUID with hardware fingerprinting for deduplication

import { invoke } from './tauri';

// ==================== Types ====================

export interface HardwareInfo {
  cpuBrand: string;
  cpuCores: number;
  machineId: string;
  osName: string;
  osVersion: string;
  hostnameHash: string;
}

export interface DeviceFingerprint {
  fingerprint: string;
  hardwareInfo: HardwareInfo;
}

export interface UserIdentity {
  userId: string;           // Anonymous UUID stored in localStorage
  deviceFingerprint: string; // Hardware-based fingerprint for deduplication
  fingerprintingEnabled: boolean; // Whether user allows fingerprinting
}

// ==================== Constants ====================

const USER_ID_KEY = 'reclaim_anonymous_user_id';
const FINGERPRINT_OPT_OUT_KEY = 'reclaim_fingerprint_opt_out';

// ==================== Anonymous User ID ====================

/**
 * Generate a random UUID v4
 */
function generateUUID(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

/**
 * Get or create the anonymous user ID from localStorage
 */
export function getAnonymousUserId(): string {
  let userId = localStorage.getItem(USER_ID_KEY);

  if (!userId) {
    userId = generateUUID();
    localStorage.setItem(USER_ID_KEY, userId);
  }

  return userId;
}

/**
 * Clear the anonymous user ID (for testing or privacy reset)
 */
export function clearAnonymousUserId(): void {
  localStorage.removeItem(USER_ID_KEY);
}

// ==================== Hardware Fingerprinting ====================

/**
 * Get hardware information from the Rust backend
 */
export async function getHardwareInfo(): Promise<HardwareInfo> {
  return await invoke<HardwareInfo>('getHardwareInfo');
}

/**
 * Get device fingerprint from the Rust backend
 */
export async function getDeviceFingerprint(): Promise<DeviceFingerprint> {
  return await invoke<DeviceFingerprint>('getDeviceFingerprint');
}

// ==================== Fingerprinting Opt-Out ====================

/**
 * Check if user has opted out of hardware fingerprinting
 */
export function isFingerprintingOptedOut(): boolean {
  return localStorage.getItem(FINGERPRINT_OPT_OUT_KEY) === 'true';
}

/**
 * Set fingerprinting opt-out status
 */
export function setFingerprintingOptOut(optOut: boolean): void {
  if (optOut) {
    localStorage.setItem(FINGERPRINT_OPT_OUT_KEY, 'true');
  } else {
    localStorage.removeItem(FINGERPRINT_OPT_OUT_KEY);
  }
}

// ==================== Combined Identity ====================

/**
 * Get the complete user identity for rating submission
 * Returns null if user has opted out of fingerprinting (they can view but not submit)
 */
export async function getUserIdentity(): Promise<UserIdentity | null> {
  const optedOut = isFingerprintingOptedOut();

  if (optedOut) {
    return null; // User cannot submit ratings if they opted out
  }

  try {
    const userId = getAnonymousUserId();
    const deviceFp = await getDeviceFingerprint();

    return {
      userId,
      deviceFingerprint: deviceFp.fingerprint,
      fingerprintingEnabled: true,
    };
  } catch (err) {
    console.error('Failed to get user identity:', err);
    return null;
  }
}

/**
 * Get identity for viewing ratings (always works, even if opted out)
 */
export function getViewingIdentity(): { userId: string; canSubmit: boolean } {
  return {
    userId: getAnonymousUserId(),
    canSubmit: !isFingerprintingOptedOut(),
  };
}

// ==================== Rating Submission Identity ====================

export interface RatingSubmissionIdentity {
  userId: string;
  deviceFingerprint: string;
}

/**
 * Get identity specifically for rating submission
 * Returns null if user cannot submit (opted out of fingerprinting)
 */
export async function getRatingSubmissionIdentity(): Promise<RatingSubmissionIdentity | null> {
  if (isFingerprintingOptedOut()) {
    return null;
  }

  try {
    const userId = getAnonymousUserId();
    const deviceFp = await getDeviceFingerprint();

    return {
      userId,
      deviceFingerprint: deviceFp.fingerprint,
    };
  } catch (err) {
    console.error('Failed to get rating submission identity:', err);
    return null;
  }
}
