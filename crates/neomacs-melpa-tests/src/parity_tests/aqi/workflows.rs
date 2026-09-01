use expect_test::expect;

use super::ParityBatchCase;

fn aqi_report_refreshes_a_real_org_buffer_with_the_latest_station_reading() -> ParityBatchCase {
    ParityBatchCase::value(
        "aqi_report_refreshes_a_real_org_buffer_with_the_latest_station_reading",
        r##"(let ((aqi-api-key "monitor-token")
               (aqi-use-cache nil)
               (aqi-cached-data '(("None" . "None")))
               (readings '((40 . "o3") (65 . "pm25")))
               calls
               first-buffer
               first-content)
         (unwind-protect
             (cl-letf
                 (((symbol-function 'request)
                   (lambda (url &rest arguments)
                     (let ((reading (pop readings)))
                       (setq calls
                             (append calls
                                     (list
                                      (list url
                                            (plist-get arguments :sync)
                                            (plist-get arguments :params)
                                            (plist-get arguments :parser)))))
                       (funcall
                        (plist-get arguments :success)
                        :data
                        `((status . "ok")
                          (data
                           . ,(aqi-test-city-data
                               "Višnjan"
                               (car reading)
                               (cdr reading))))))
                     :network-response)))
               (aqi-report "Višnjan" 'full)
               (setq first-buffer (get-buffer "*Air Quality - Višnjan*")
                     first-content
                     (with-current-buffer first-buffer
                       (buffer-string)))
               (aqi-report "Višnjan" 'brief)
               (let* ((second-buffer (get-buffer "*Air Quality - Višnjan*"))
                      (window (get-buffer-window second-buffer t)))
                 (list
                  first-content
                  (eq first-buffer second-buffer)
                  (with-current-buffer second-buffer
                    (list
                     (buffer-string)
                     major-mode
                     (= (point) (point-max))))
                  (and window
                       (buffer-name (window-buffer window)))
                  calls
                  readings)))
           (aqi-test-kill-report-buffers)))"##,
        expect![[
            r#"OK ("* Air Quality index in Višnjan is 40\n\nMost recent report at 2023-05-30 12:00:00 (UTC+02:00).\n\n| Dominant pollutant                   |   o3 |\n| PM2.5 (fine particulate matter)      |   12 |\n| PM10 (respirable particulate matter) |   21 |\n| NO2 (Nitrogen Dioxide)               |   7 |\n| CO (Carbon Monoxide)                 |   3 |\n|                                      |    |\n| Temperature (Celsius)                |   24 |\n| Humidity                             |   61 |\n| Air pressure                         |   1014 |\n| Wind                                 |   5 |\n\nFurther details can be found at [[https://aqicn.example/station][aqicn]].\n\nData provided by World Air Quality Index and Local Sensor Network" t ("Air Quality Index in Višnjan is 65 and the dominant pollutant is pm25" org-mode t) "*Air Quality - Višnjan*" (("https://api.waqi.info/feed/Višnjan/" t (("token" . "monitor-token")) json-read) ("https://api.waqi.info/feed/Višnjan/" t (("token" . "monitor-token")) json-read)) nil)"#
        ]],
    )
}

fn aqi_cached_dashboard_reuses_each_station_reading_without_another_request() -> ParityBatchCase {
    ParityBatchCase::value(
        "aqi_cached_dashboard_reuses_each_station_reading_without_another_request",
        r##"(let ((aqi-api-key "dashboard-token")
               (aqi-use-cache t)
               (aqi-cached-data '(("None" . "None")))
               calls)
         (cl-letf
             (((symbol-function 'request)
               (lambda (url &rest arguments)
                 (setq calls (append calls (list url)))
                 (let* ((taipei (string-match-p "Taipei" url))
                        (city (if taipei "Taipei" "Station 7397"))
                        (score (if taipei 17 54))
                        (pollutant (if taipei "pm25" "pm10")))
                   (funcall
                    (plist-get arguments :success)
                    :data
                    `((status . "ok")
                      (data
                       . ,(aqi-test-city-data city score pollutant)))))
                 :network-response)))
           (let ((first-taipei (aqi-report-brief "Taipei"))
                 (second-taipei (aqi-report-brief "Taipei"))
                 (first-station (aqi-report-brief "@7397"))
                 (second-station (aqi-report-brief "@7397")))
             (list
              first-taipei
              second-taipei
              first-station
              second-station
              calls))))"##,
        expect![[
            r#"OK ("Air Quality Index in Taipei is 17 and the dominant pollutant is pm25 (cached)" "Air Quality Index in Taipei is 17 and the dominant pollutant is pm25 (cached)" "Air Quality Index in Station 7397 is 54 and the dominant pollutant is pm10 (cached)" "Air Quality Index in Station 7397 is 54 and the dominant pollutant is pm10 (cached)" ("https://api.waqi.info/feed/Taipei/" "https://api.waqi.info/feed/@7397/"))"#
        ]],
    )
}

fn aqi_accessors_select_the_cleanest_destination_and_render_its_summary() -> ParityBatchCase {
    ParityBatchCase::value(
        "aqi_accessors_select_the_cleanest_destination_and_render_its_summary",
        r##"(let ((aqi-use-cache nil)
               (aqi-cached-data '(("None" . "None")))
               calls)
         (cl-letf
             (((symbol-function 'request)
               (lambda (url &rest arguments)
                 (setq calls (append calls (list url)))
                 (let* ((city (cond
                               ((string-match-p "Osaka" url) "Osaka")
                               ((string-match-p "Taipei" url) "Taipei")
                               (t "New Delhi")))
                        (score (cond
                                ((equal city "Osaka") 42)
                                ((equal city "Taipei") 17)
                                (t 73)))
                        (pollutant (if (equal city "New Delhi") "pm10" "pm25")))
                   (funcall
                    (plist-get arguments :success)
                    :data
                    `((status . "ok")
                      (data
                       . ,(aqi-test-city-data city score pollutant)))))
                 :network-response)))
           (let* ((readings
                   (mapcar
                    (lambda (city)
                      (cons city (aqi-city-aqi city)))
                    '("Osaka" "Taipei" "New Delhi")))
                  (ranked
                   (sort
                    (copy-tree readings)
                    (lambda (left right)
                      (< (cdr left) (cdr right)))))
                  (winner (car ranked))
                  (city (car winner)))
             (list
              readings
              ranked
              (aqi-city-lonlat city)
              (aqi-report-brief city)
              calls))))"##,
        expect![[
            r#"OK ((("Osaka" . 42) ("Taipei" . 17) ("New Delhi" . 73)) (("Taipei" . 17) ("Osaka" . 42) ("New Delhi" . 73)) "45.274, 13.721" "Air Quality Index in Taipei is 17 and the dominant pollutant is pm25" ("https://api.waqi.info/feed/Osaka/" "https://api.waqi.info/feed/Taipei/" "https://api.waqi.info/feed/New Delhi/" "https://api.waqi.info/feed/Taipei/" "https://api.waqi.info/feed/Taipei/"))"#
        ]],
    )
}

fn aqi_programmatic_report_recovers_from_an_unknown_station_on_the_next_fetch() -> ParityBatchCase {
    ParityBatchCase::value(
        "aqi_programmatic_report_recovers_from_an_unknown_station_on_the_next_fetch",
        r##"(let ((aqi-use-cache nil)
               (aqi-cached-data '(("None" . "None")))
               (attempt 0)
               calls)
         (cl-letf
             (((symbol-function 'request)
               (lambda (url &rest arguments)
                 (setq attempt (1+ attempt)
                       calls (append calls (list url)))
                 (funcall
                  (plist-get arguments :success)
                  :data
                  (if (= attempt 1)
                      '((status . "error")
                        (data . "Unknown station"))
                    `((status . "ok")
                      (data
                       . ,(aqi-test-city-data
                           "Central Station"
                           28
                           "pm25")))))
                 :network-response)))
           (list
            (aqi-report-full "@missing")
            (aqi-report-full "@missing")
            calls
            attempt)))"##,
        expect![[
            r#"OK ("Request error: Unknown station (@missing)" "* Air Quality index in Central Station is 28\n\nMost recent report at 2023-05-30 12:00:00 (UTC+02:00).\n\n| Dominant pollutant                   |   pm25 |\n| PM2.5 (fine particulate matter)      |   12 |\n| PM10 (respirable particulate matter) |   21 |\n| NO2 (Nitrogen Dioxide)               |   7 |\n| CO (Carbon Monoxide)                 |   3 |\n|                                      |    |\n| Temperature (Celsius)                |   24 |\n| Humidity                             |   61 |\n| Air pressure                         |   1014 |\n| Wind                                 |   5 |\n\nFurther details can be found at [[https://aqicn.example/station][aqicn]].\n\nData provided by World Air Quality Index and Local Sensor Network" ("https://api.waqi.info/feed/@missing/" "https://api.waqi.info/feed/@missing/") 2)"#
        ]],
    )
}

fn aqi_station_search_and_geo_lookup_surface_success_and_transport_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "aqi_station_search_and_geo_lookup_surface_success_and_transport_failure",
        r##"(let* ((aqi-api-key "field-token")
               (message-log-max t)
               (message-buffer (get-buffer-create "*Messages*"))
               (message-start
                (with-current-buffer message-buffer
                  (point-max)))
               calls
               search-message
               geo-message)
         (cl-letf
             (((symbol-function 'request)
               (lambda (url &rest arguments)
                 (setq calls
                       (append calls
                               (list
                                (list url
                                      (plist-get arguments :sync)
                                      (plist-get arguments :params)
                                      (plist-get arguments :parser)))))
                 (cond
                  ((string-match-p "New Delhi" url)
                   (funcall
                    (plist-get arguments :success)
                    :data
                    '((status . "ok")
                      (data
                       . [((station
                            (name . "Delhi Central")
                            (uid . 7397))
                           (aqi . 54))]))))
                  ((string-match-p "/geo:" url)
                   (funcall
                    (plist-get arguments :success)
                    :data
                    '((status . "ok")
                      (data
                       (aqi . 51)
                       (city
                        (name . "Sydney"))))))
                  (t
                   (funcall
                    (plist-get arguments :error)
                    :error-thrown
                    '(file-error "network unreachable"))))
                 :network-response)))
           (aqi-search "New Delhi")
           (setq search-message
                 (with-current-buffer message-buffer
                   (buffer-substring-no-properties
                    message-start
                    (point-max)))
                 message-start
                 (with-current-buffer message-buffer
                   (point-max)))
           (aqi-request-geo -33.8688 151.2093)
           (setq geo-message
                 (with-current-buffer message-buffer
                   (buffer-substring-no-properties
                    message-start
                    (point-max)))
                 message-start
                 (with-current-buffer message-buffer
                   (point-max)))
           (aqi-search "Offline")
           (list
            search-message
            geo-message
            (with-current-buffer message-buffer
              (buffer-substring-no-properties
               message-start
               (point-max)))
            calls)))"##,
        expect![[
            r#"OK ("Search: [((station (name . Delhi Central) (uid . 7397)) (aqi . 54))]\n" "200: ((status . ok) (data (aqi . 51) (city (name . Sydney))))\n" "WAQI error: (file-error network unreachable)\n" (("https://api.waqi.info/search/?keyword=New Delhi&" t (("token" . "field-token")) json-read) ("https://api.waqi.info/feed/geo:-33.8688;151.2093/" t (("token" . "field-token")) json-read) ("https://api.waqi.info/search/?keyword=Offline&" t (("token" . "field-token")) json-read)))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aqi_report_refreshes_a_real_org_buffer_with_the_latest_station_reading(),
        aqi_cached_dashboard_reuses_each_station_reading_without_another_request(),
        aqi_accessors_select_the_cleanest_destination_and_render_its_summary(),
        aqi_programmatic_report_recovers_from_an_unknown_station_on_the_next_fetch(),
        aqi_station_search_and_geo_lookup_surface_success_and_transport_failure(),
    ]
}
