// `node dist/main.js express|fastify [port] [workers]`: serve the NestJS version on 127.0.0.1.
// With workers > 1 the primary forks that many processes with node:cluster, all on the same port.
import 'reflect-metadata';
import cluster from 'node:cluster';

import { NestFactory } from '@nestjs/core';

import { BenchModule, validationPipe } from './app';

async function serve(adapter: string, port: number): Promise<void> {
  // load only the adapter in use, so its memory is all the process carries
  const platform =
    adapter === 'fastify'
      ? new (await import('@nestjs/platform-fastify')).FastifyAdapter()
      : new (await import('@nestjs/platform-express')).ExpressAdapter();
  const app = await NestFactory.create(BenchModule, platform, { logger: false });
  app.useGlobalPipes(validationPipe());
  await app.listen(port, '127.0.0.1');
}

const [adapter = 'express', portArg = '3000', workersArg = '1'] = process.argv.slice(2);
if (adapter !== 'express' && adapter !== 'fastify') {
  console.error('usage: node dist/main.js express|fastify [port] [workers]');
  process.exit(2);
}
const port = Number(portArg);
const workers = Number(workersArg);

if (workers > 1 && cluster.isPrimary) {
  for (let i = 0; i < workers; i += 1) cluster.fork();
  cluster.on('exit', () => process.exit(1));
  console.log(`nest-${adapter} listening on http://127.0.0.1:${port} with ${workers} workers`);
} else {
  serve(adapter, port).then(
    () => {
      if (workers <= 1) console.log(`nest-${adapter} listening on http://127.0.0.1:${port}`);
    },
    (err) => {
      console.error(err);
      process.exit(1);
    },
  );
}
