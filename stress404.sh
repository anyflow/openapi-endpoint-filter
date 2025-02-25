#!/bin/bash

URL="https://api.anyflow.net/dockebi/v1/stuff/123/child/456/hello"
INITIAL_QPS=10   # 시작 QPS
MAX_QPS=1000     # 최대 QPS (제한)
THREADS=4        # 동시 실행 스레드 수
DURATION=10s     # 부하 지속 시간
THRESHOLD_LATENCY=0.5  # 평균 응답 시간(초) 한계
THRESHOLD_ERROR=5      # 에러 비율 한계 (%)

qps=$INITIAL_QPS
while [ $qps -le $MAX_QPS ]; do
    echo "Running test with QPS: $qps"

    # Fortio 실행 (404 응답 포함)
    fortio load -qps $qps -c $THREADS -t $DURATION -json result.json -allow-initial-errors $URL

    # JSON에서 평균 응답 시간 추출
    avg_latency=$(jq '.DurationHistogram.Avg' result.json)

    # HTTP 404, 500, 503 에러 발생 횟수 추출
    error_404=$(jq '[.RetCodes | to_entries[] | select(.key == "404") | .value] | add // 0' result.json)
    error_500=$(jq '[.RetCodes | to_entries[] | select(.key | startswith("500")) | .value] | add // 0' result.json)
    error_503=$(jq '[.RetCodes | to_entries[] | select(.key | startswith("503")) | .value] | add // 0' result.json)
    total_requests=$(jq '.DurationHistogram.Count' result.json)

    # 404는 정상 응답으로 간주하고, 500/503만 에러 비율 계산
    error_count=$(( error_500 + error_503 ))
    error_rate=$(awk "BEGIN {if ($total_requests > 0) print ($error_count / $total_requests) * 100; else print 0}")

    echo "Avg Latency: ${avg_latency}s, 404 Responses: ${error_404}, Error Rate: ${error_rate}%"

    # 한계 초과 시 종료 또는 부하 감소
    if (( $(echo "$avg_latency > $THRESHOLD_LATENCY" | bc -l) )) || (( $(echo "$error_rate > $THRESHOLD_ERROR" | bc -l) )); then
        echo "Threshold exceeded! Reducing load..."
        qps=$(( qps / 2 ))
        if [ $qps -lt $INITIAL_QPS ]; then
            echo "Found optimal QPS: $qps"
            exit 0
        fi
    else
        # 부하 증가
        qps=$(( qps * 2 ))
    fi

    sleep 5  # 부하 테스트 간격 조정
done
