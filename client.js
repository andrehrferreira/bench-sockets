const WebSocket = require('ws');
const net = require('net');
const dgram = require('dgram'); 

const env = process.env;
const SERVERS = [
    //{ name: "Rust WebSocket", url: "ws://127.0.0.1:3001", protocol: 'ws' },
    //{ name: "Rust TCP", url: "127.0.0.1:4001", protocol: 'tcp' },
    { name: "Rust UDP", url: "127.0.0.1:5001", protocol: 'udp' },
];

const LOG_MESSAGES = env.LOG_MESSAGES === "1";
const CLIENTS_TO_WAIT_FOR = 100;
const DELAY = 64;
const WAIT_TIME_BETWEEN_TESTS = 5000;
const MESSAGES_TO_SEND = [
  "Hello World!",
  "Hello World! 1",
  "Hello World! 2",
  "Hello World! 3",
  "Hello World! 4",
  "Hello World! 5",
  "Hello World! 6",
  "Hello World! 7",
  "Hello World! 8",
  "Hello World! 9",
  "What is the meaning of life?",
  "where is the bathroom?",
  "zoo",
  "kangaroo",
  "erlang",
  "elixir",
  "bun",
  "mochi",
  "typescript",
  "javascript"
];

const NAMES = Array.from({ length: CLIENTS_TO_WAIT_FOR }, (a, i) => `Client${i}`);

const results = [];

async function testServer(server) {
  console.log(`Connecting to ${server.name} at ${server.url}`);
  console.time(`All clients connected to ${server.name}`);
  let promises = [];
  let received = 0;
  let lostPackets = 0;

  const clients = new Array(CLIENTS_TO_WAIT_FOR);

  for (let i = 0; i < CLIENTS_TO_WAIT_FOR; i++) {
    if (server.protocol === 'ws') {
      clients[i] = new WebSocket(`${server.url}?name=${NAMES[i]}`);

      clients[i].on('message', (event) => {
        if (LOG_MESSAGES) console.log(event);
        received++;
      });
    } else if (server.protocol === 'tcp') {
      clients[i] = new net.Socket();

      let buffer = '';
      clients[i].on('data', (data) => {
        buffer += data.toString();
        let parts = buffer.split('\n');
        buffer = parts.pop(); // Mantém a parte incompleta no buffer
        for (const message of parts) {
          if (LOG_MESSAGES) console.log(message);
          received++;
        }
      });
      
      clients[i].connect(4001, '127.0.0.1');

    } else if (server.protocol === 'udp') {
      clients[i] = dgram.createSocket('udp4');
      
      clients[i].on('listening', () => {
        clients[i].setRecvBufferSize(1024 * 1024); // 1 MB de buffer de recebimento
        clients[i].setSendBufferSize(1024 * 1024); // 1 MB de buffer de envio
      });

      clients[i].on('message', (msg, rinfo) => {
        if (LOG_MESSAGES) console.log(`UDP client received: ${msg} from ${rinfo.address}:${rinfo.port}`);
        received++;
      });

      clients[i].on('error', (err) => {
        console.error(`Client error: ${err.stack}`);
        clients[i].close(); // Fechando o socket em caso de erro
      });

      clients[i].bind();
    }
  }

  function sendMessagesContinuously() {
    for (let i = 0; i < CLIENTS_TO_WAIT_FOR; i++) {
      for (let j = 0; j < MESSAGES_TO_SEND.length; j++) {
        if (server.protocol === 'ws') {
          if (clients[i].readyState === WebSocket.OPEN)
            clients[i].send(MESSAGES_TO_SEND[j]);
        } else if (server.protocol === 'tcp') {
          if (clients[i].readyState === 'open')
            clients[i].write(MESSAGES_TO_SEND[j] + '\n');
        } else if (server.protocol === 'udp') {
          if (clients[i] && clients[i]._handle) {
            clients[i].send(MESSAGES_TO_SEND[j], 5001, '127.0.0.1', (err) => {
              if (err) {
                console.error(`Error sending message: ${err.message}`);
                lostPackets++;
              }
            });
          }
        }
      }
    }
  }

  const runs = [];
  await new Promise((resolve) => {
    const interval = setInterval(() => {
      const last = received;
      if (last === 0) {
        lostPackets++;
      } else if (last > 0) {
        runs.push(last);
      }
      received = 0;
      console.log(
        `${server.name}: ${last} messages per second (${CLIENTS_TO_WAIT_FOR} clients x ${MESSAGES_TO_SEND.length} msg, min delay: ${DELAY}ms)`
      );

      if (runs.length >= 5) {
        console.log(`${server.name}: 5 runs completed`);
        clearInterval(interval);
        resolve();
      }
    }, 1000);

    sendMessagesContinuously();
    setInterval(sendMessagesContinuously, DELAY);
  });

  const sum = runs.reduce((acc, val) => acc + val, 0);
  const average = sum / runs.length;
  console.log(`Average messages per second for ${server.name}: ${average}`);
  results.push({ name: server.name, average, lostPackets });

  // Fechar todas as conexões depois do teste
  for (let i = 0; i < CLIENTS_TO_WAIT_FOR; i++) {
    if (server.protocol === 'ws') 
      clients[i].close();
    else if (server.protocol === 'tcp') 
      clients[i].end();
    else if (server.protocol === 'udp') 
      clients[i].close();
  }
}

async function runTests() {
  for (const server of SERVERS) {
    await testServer(server);
    await new Promise((resolve) => setTimeout(resolve, WAIT_TIME_BETWEEN_TESTS));
  }

  const overallAverage = results.reduce((acc, { average }) => acc + average, 0) / results.length;

  results.forEach(result => {
    result.percentage = ((result.average - overallAverage) / overallAverage) * 100;
  });

  results.sort((a, b) => b.average - a.average);

  console.table(results.map(result => ({
    Server: result.name,
    "Avg Messages/sec": result.average,
    "Lost Packets": result.lostPackets,
    "% Difference": `${result.percentage.toFixed(2)}%`
  })));
}

runTests();
