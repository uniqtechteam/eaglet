// The NestJS version: one module, two controllers, one guard, the way a Nest app is usually written.
import {
  Body,
  CanActivate,
  Controller,
  ExecutionContext,
  Get,
  HttpCode,
  Injectable,
  Module,
  Param,
  ParseIntPipe,
  Post,
  Req,
  UnauthorizedException,
  UnprocessableEntityException,
  UseGuards,
  ValidationError,
  ValidationPipe,
} from '@nestjs/common';

import { NewUser, User, Users } from './users';

/** Lets requests with `Authorization: Bearer bench-token` through and puts the user on them. */
@Injectable()
export class AuthGuard implements CanActivate {
  constructor(private readonly users: Users) {}

  canActivate(context: ExecutionContext): boolean {
    const req = context.switchToHttp().getRequest<{ headers: Record<string, string>; user?: User }>();
    const user = this.users.byToken(req.headers.authorization);
    if (!user) throw new UnauthorizedException({ code: 'UNAUTHORIZED', message: 'sign in first' });
    req.user = user;
    return true;
  }
}

@Controller('users')
export class UsersController {
  constructor(private readonly users: Users) {}

  @Get(':id')
  find(@Param('id', ParseIntPipe) id: number): User {
    return this.users.find(id);
  }

  @Post()
  @HttpCode(201)
  create(@Body() input: NewUser): User {
    return this.users.create(input);
  }
}

@Controller('me')
@UseGuards(AuthGuard)
export class MeController {
  @Get()
  me(@Req() req: { user: User }): User {
    return req.user;
  }
}

@Module({
  controllers: [UsersController, MeController],
  providers: [Users, AuthGuard],
})
export class BenchModule {}

/** The global pipe: 422 with the same `code` the Rust versions send. */
export function validationPipe(): ValidationPipe {
  return new ValidationPipe({
    exceptionFactory: (errors: ValidationError[]) =>
      new UnprocessableEntityException({
        code: 'VALIDATION_FAILED',
        message: `some fields are not valid: ${errors.map((e) => e.property).join(', ')}`,
      }),
  });
}
