import { Request, Response, NextFunction } from 'express';
import { AppError } from '../utils/errors';
import { logger } from '../logger';

interface ErrorResponse {
  code: string;
  message: string;
  details?: any;
  timestamp: string;
  path: string;
}

/**
 * Centralized error handling middleware
 * Normalizes all errors to consistent format
 */
export function errorHandler(
  err: Error | AppError,
  req: Request,
  res: Response,
  next: NextFunction
) {
  // Log error for debugging
  logger.error(err.message, err instanceof Error ? err : undefined, {
    name: err.name,
    stack: process.env.NODE_ENV === 'development' ? err.stack : undefined,
    path: req.path,
    method: req.method,
    ip: req.ip,
  });

  // Default error response
  let errorResponse: ErrorResponse = {
    code: 'INTERNAL_SERVER_ERROR',
    message: 'Something went wrong',
    timestamp: new Date().toISOString(),
    path: req.path
  };
  let statusCode = 500;

  // Handle known AppError instances
  if (err instanceof AppError) {
    errorResponse = {
      code: err.code,
      message: err.message,
      details: err.details,
      timestamp: new Date().toISOString(),
      path: req.path
    };
    statusCode = err.statusCode;
    return res.status(statusCode).json(errorResponse);
  }

  // Handle Zod validation errors
  if (err.name === 'ZodError') {
    const zodErr = err as any;
    errorResponse = {
      code: 'VALIDATION_ERROR',
      message: 'Validation failed',
      details: zodErr.errors?.map((e: any) => ({
        field: e.path.join('.'),
        message: e.message
      })) || zodErr.message,
      timestamp: new Date().toISOString(),
      path: req.path
    };
    return res.status(422).json(errorResponse);
  }

  // Handle PostgreSQL errors
  if ((err as any).code && typeof (err as any).code === 'string') {
    const pgErr = err as any;
    switch (pgErr.code) {
      case '23505': // unique violation
        errorResponse = {
          code: 'DUPLICATE_ERROR',
          message: 'A record with this value already exists',
          details: pgErr.detail,
          timestamp: new Date().toISOString(),
          path: req.path
        };
        return res.status(409).json(errorResponse);
      case '23503': // foreign key violation
        errorResponse = {
          code: 'FOREIGN_KEY_ERROR',
          message: 'Referenced record not found',
          details: pgErr.detail,
          timestamp: new Date().toISOString(),
          path: req.path
        };
        return res.status(400).json(errorResponse);
      case '42P01': // undefined table
        errorResponse = {
          code: 'DATABASE_ERROR',
          message: 'Database configuration error',
          timestamp: new Date().toISOString(),
          path: req.path
        };
        return res.status(500).json(errorResponse);
    }
  }

  res.status(statusCode).json(errorResponse);
}

/**
 * Async wrapper to catch errors in route handlers
 * Use this to avoid try-catch blocks in controllers
 */
export function asyncHandler(fn: Function) {
  return (req: Request, res: Response, next: NextFunction) => {
    Promise.resolve(fn(req, res, next)).catch(next);
  };
}
