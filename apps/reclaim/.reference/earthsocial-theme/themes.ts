import { Router } from 'express';
import { authenticate } from '../middleware/auth.js';
import { validateBody } from '@/middleware/validate.js';
import { z } from 'zod';
import db, { unwrap, unwrapOrNull } from '../utils/db.js';

const router = Router();

// Schema for theme data
const ThemeDataSchema = z.object({
  baseTheme: z.string().optional(), // Which preset theme this is based on (ocean-turtle, air-clouds, etc.)
  customization: z.object({
    primaryColor: z.string(),
    secondaryColor: z.string(),
    accentColor: z.string(),
    highlightColor: z.string().optional(),
    textColor: z.string(),
    navbarBg: z.string().optional(),
    tabBarBg: z.string().optional(),
    navbarOpacity: z.number().optional(),
    dropdownColor: z.string().optional(),
    dropdownOpacity: z.number().optional(),
    dropdownButtonOpacity: z.number().optional(),
    cardBg: z.string(),
    cardGradientEnabled: z.boolean(),
    cardGradientAngle: z.number(),
    cardGradientColor1: z.string(),
    cardGradientColor2: z.string(),
    cardGradientFavorability: z.number().optional(),
    cardOpacity: z.number().optional(),
    cardGradientStrength: z.number().optional(),
    gradientEnabled: z.boolean(),
    gradientAngle: z.number(),
    gradientFrom: z.string().optional(),
    gradientTo: z.string().optional(),
    gradientFavorability: z.number().optional(),
    gradientStrength: z.number().optional(),
    bubbleColor: z.string(),
    turtleColor: z.string(),
    coralColors: z.array(z.string()),
    profileFrameColor: z.string(),
    profileFrameShape: z.enum(['square', 'circle']),
    profileNameAlign: z.enum(['left', 'center', 'right']),
    profileFrameDesign: z.enum(['coral', 'waves', 'bubbles']),
    animationsEnabled: z.boolean(),
    selectedCharacter: z.string(),
    selectedDecoration: z.string(),
    focusAnimationEnabled: z.boolean().optional(),
    presetTheme: z.string().optional(),
    customPresets: z.record(z.object({
      name: z.string(),
      settings: z.any(), // Contains full theme customization + animations
    })).optional(),
    // Midnight Lizard-inspired background options
    backgroundSaturationLimit: z.number().optional(),
    backgroundContrast: z.number().optional(),
    backgroundBrightnessLimit: z.number().optional(),
    backgroundGraySaturation: z.number().optional(),
    backgroundHueGravity: z.number().optional(),
    backgroundDefaultHue: z.number().optional(),
    // Color space selection and transformation options
    colorSpace: z.enum(['Off', 'RGB', 'HSV', 'TMI']).optional(),
    // TMI options
    temperatureLimit: z.number().optional(),
    magentaLimit: z.number().optional(),
    intensityLimit: z.number().optional(),
    // RGB options
    redLimit: z.number().optional(),
    greenLimit: z.number().optional(),
    blueLimit: z.number().optional(),
  }),
  animations: z.object({
    characters: z.array(z.object({
      id: z.string(),
      enabled: z.boolean(),
      speed: z.number(),
    })),
    decorations: z.array(z.object({
      id: z.string(),
      enabled: z.boolean(),
      speed: z.number(),
    })),
    bubbles: z.object({
      enabled: z.boolean(),
    }).nullable(),
  }),
  animationsEnabled: z.boolean(),
});

/**
 * GET /users/:userId/site-theme - Get user's site-wide theme
 */
router.get('/users/:userId/site-theme', authenticate, async (req, res, next) => {
  try {
    const userId = parseInt(req.params.userId, 10);

    if (!Number.isFinite(userId)) {
      return res.status(400).json({ error: 'Invalid user ID' });
    }

    if (req.user!.id !== userId) {
      return res.status(403).json({ error: 'Can only view your own theme' });
    }

    const user = unwrapOrNull(await db.user.findUnique({
      where: { id: userId },
      select: {
        siteTheme: true,
      },
    }));

    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    // Cast to any - include types not inferred (see INCLUDE_ERRORS_AUDIT.md)
    res.json((user as any).siteTheme?.tokens || {});
  } catch (error) {
    next(error);
  }
});

/**
 * PUT /users/:userId/site-theme - Update user's site-wide theme
 */
router.put('/users/:userId/site-theme', authenticate, validateBody(ThemeDataSchema), async (req, res, next) => {
  try {
    const userId = parseInt(req.params.userId, 10);

    if (!Number.isFinite(userId)) {
      return res.status(400).json({ error: 'Invalid user ID' });
    }

    if (req.user!.id !== userId) {
      return res.status(403).json({ error: 'Can only update your own theme' });
    }

    const themeData = req.body;

    // Create or update theme record
    const theme = unwrap(await db.theme.upsert({
      where: {
        name_scope_isPreset: {
          name: `user_${userId}_site`,
          scope: 'site',
          isPreset: false,
        },
      },
      update: {
        tokens: themeData as any,
        updatedAt: new Date(),
      },
      create: {
        name: `user_${userId}_site`,
        scope: 'site',
        isPreset: false,
        tokens: themeData as any,
      },
    }));

    // Link theme to user
    unwrap(await db.user.update({
      where: { id: userId },
      data: {
        siteThemeId: theme.id,
      },
    }));

    res.json({
      message: 'Site theme updated successfully',
      theme: theme.tokens,
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /users/:userId/chat-theme - Get user's chat theme
 */
router.get('/users/:userId/chat-theme', authenticate, async (req, res, next) => {
  try {
    const userId = parseInt(req.params.userId, 10);

    if (!Number.isFinite(userId)) {
      return res.status(400).json({ error: 'Invalid user ID' });
    }

    if (req.user!.id !== userId) {
      return res.status(403).json({ error: 'Can only view your own theme' });
    }

    const user = unwrapOrNull(await db.user.findUnique({
      where: { id: userId },
      select: {
        chatTheme: true,
      },
    }));

    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }

    // Cast to any - include types not inferred (see INCLUDE_ERRORS_AUDIT.md)
    res.json((user as any).chatTheme?.tokens || {});
  } catch (error) {
    next(error);
  }
});

/**
 * PUT /users/:userId/chat-theme - Update user's chat theme
 */
router.put('/users/:userId/chat-theme', authenticate, validateBody(ThemeDataSchema), async (req, res, next) => {
  try {
    const userId = parseInt(req.params.userId, 10);

    if (!Number.isFinite(userId)) {
      return res.status(400).json({ error: 'Invalid user ID' });
    }

    if (req.user!.id !== userId) {
      return res.status(403).json({ error: 'Can only update your own theme' });
    }

    const themeData = req.body;

    // Create or update theme record
    const theme = unwrap(await db.theme.upsert({
      where: {
        name_scope_isPreset: {
          name: `user_${userId}_chat`,
          scope: 'messages',
          isPreset: false,
        },
      },
      update: {
        tokens: themeData as any,
        updatedAt: new Date(),
      },
      create: {
        name: `user_${userId}_chat`,
        scope: 'messages',
        isPreset: false,
        tokens: themeData as any,
      },
    }));

    // Link theme to user
    unwrap(await db.user.update({
      where: { id: userId },
      data: {
        chatThemeId: theme.id,
      },
    }));

    res.json({
      message: 'Chat theme updated successfully',
      theme: theme.tokens,
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /communities/:communityId/theme - Get community theme
 */
router.get('/communities/:communityId/theme', async (req, res, next) => {
  try {
    const { communityId } = req.params;

    const community = unwrapOrNull(await db.community.findUnique({
      where: { id: communityId },
      select: {
        theme: true,
      },
    }));

    if (!community) {
      return res.status(404).json({ error: 'Community not found' });
    }

    // Cast to any - include types not inferred (see INCLUDE_ERRORS_AUDIT.md)
    res.json((community as any).theme?.tokens || {});
  } catch (error) {
    next(error);
  }
});

/**
 * PUT /communities/:communityId/theme - Update community theme (owner/admin only)
 */
router.put('/communities/:communityId/theme', authenticate, validateBody(ThemeDataSchema), async (req, res, next) => {
  try {
    const { communityId } = req.params;

    // Check if user is owner or admin of the community
    const membership = unwrapOrNull(await db.communityMember.findUnique({
      where: {
        communityId_userId: {
          communityId,
          userId: req.user!.id,
        },
      },
    }));

    if (!membership || !['owner', 'admin'].includes(membership.role)) {
      return res.status(403).json({ error: 'Only community owners and admins can update the theme' });
    }

    const themeData = req.body;

    // Create or update theme record
    const theme = unwrap(await db.theme.upsert({
      where: {
        name_scope_isPreset: {
          name: `community_${communityId}`,
          scope: 'community',
          isPreset: false,
        },
      },
      update: {
        tokens: themeData as any,
        updatedAt: new Date(),
      },
      create: {
        name: `community_${communityId}`,
        scope: 'community',
        isPreset: false,
        tokens: themeData as any,
      },
    }));

    // Link theme to community
    unwrap(await db.community.update({
      where: { id: communityId },
      data: {
        themeId: theme.id,
      },
    }));

    res.json({
      message: 'Community theme updated successfully',
      theme: theme.tokens,
    });
  } catch (error) {
    next(error);
  }
});

export default router;
