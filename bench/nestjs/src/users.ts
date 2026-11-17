// What the routes share: the DTOs and the service. The same as bench/src/shared.rs.
import { Injectable } from '@nestjs/common';
import { IsEmail, Length } from 'class-validator';

/** The token `GET /me` accepts. */
export const TOKEN = 'bench-token';

/** A user as the API returns it. */
export interface User {
  id: number;
  name: string;
  email: string;
}

/** The body of `POST /users`: the same rules as the Rust `NewUser`. */
export class NewUser {
  @Length(1, 64)
  name!: string;

  @IsEmail()
  email!: string;
}

/** The work behind the routes. Kept tiny on purpose: the bench measures the framework, not this. */
@Injectable()
export class Users {
  find(id: number): User {
    return { id, name: `user ${id}`, email: `user${id}@example.com` };
  }

  create(input: NewUser): User {
    return { id: 1, name: input.name, email: input.email };
  }

  byToken(authorization: string | undefined): User | undefined {
    if (!authorization?.startsWith('Bearer ')) return undefined;
    return authorization.slice('Bearer '.length) === TOKEN ? this.find(7) : undefined;
  }
}
