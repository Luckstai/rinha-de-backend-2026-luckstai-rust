import http from 'k6/http';
import { SharedArray } from 'k6/data';
import { Counter } from 'k6/metrics';
import exec from 'k6/execution';

const targetRps = Number(__ENV.TARGET_RPS || 200);
const duration = __ENV.DURATION || '30s';
const baseUrl = __ENV.TARGET_BASE_URL || 'http://localhost:9999';
const preAllocatedVUs = Number(__ENV.PREALLOCATED_VUS || Math.max(50, targetRps));
const maxVUs = Number(__ENV.MAX_VUS || Math.max(250, targetRps * 2));

const testData = new SharedArray('test-data', function () {
    return JSON.parse(open('./test-data.json')).entries;
});

const tpCount = new Counter('tp_count');
const tnCount = new Counter('tn_count');
const fpCount = new Counter('fp_count');
const fnCount = new Counter('fn_count');
const errorCount = new Counter('error_count');

export const options = {
    summaryTrendStats: ['avg', 'p(95)', 'p(99)'],
    systemTags: ['status', 'method'],
    dns: {
        ttl: '5m',
        select: 'roundRobin',
    },
    scenarios: {
        default: {
            executor: 'constant-arrival-rate',
            rate: targetRps,
            timeUnit: '1s',
            duration,
            preAllocatedVUs,
            maxVUs,
            gracefulStop: '5s',
        },
    },
};

export function setup() {
    console.log(`Calibration run: target=${targetRps} rps duration=${duration} baseUrl=${baseUrl} dataset=${testData.length}`);
}

export default function () {
    const idx = exec.scenario.iterationInTest % testData.length;
    const entry = testData[idx];
    const expectedApproved = entry.expected_approved;

    const res = http.post(
        `${baseUrl}/fraud-score`,
        JSON.stringify(entry.request),
        { headers: { 'Content-Type': 'application/json' }, timeout: '2001ms' }
    );

    if (res.status === 200) {
        const body = JSON.parse(res.body);
        if (expectedApproved === body.approved) {
            if (body.approved) tnCount.add(1);
            else tpCount.add(1);
        } else {
            if (body.approved) fnCount.add(1);
            else fpCount.add(1);
        }
    } else {
        errorCount.add(1);
    }
}

export function handleSummary(data) {
    const result = {
        target_rps: targetRps,
        duration,
        base_url: baseUrl,
        http_req_duration: data.metrics.http_req_duration.values,
        http_req_failed: data.metrics.http_req_failed ? data.metrics.http_req_failed.values : null,
        checks: data.metrics.checks ? data.metrics.checks.values : null,
        detections: {
            tp: data.metrics.tp_count ? data.metrics.tp_count.values.count : 0,
            tn: data.metrics.tn_count ? data.metrics.tn_count.values.count : 0,
            fp: data.metrics.fp_count ? data.metrics.fp_count.values.count : 0,
            fn: data.metrics.fn_count ? data.metrics.fn_count.values.count : 0,
            http_errors: data.metrics.error_count ? data.metrics.error_count.values.count : 0,
        },
    };

    return {
        'test/results.calibrate.json': JSON.stringify(result, null, 2),
    };
}
