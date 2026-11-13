/* Rust host bridge for the OHDSI DataQualityDashboard v2.9.0 browser assets.
 * Apache-2.0. The original components retain their own upstream licenses. */
(function () {
    'use strict';
    const toolbar = document.createElement('div');
    toolbar.className = 'dqd-toolbar';
    toolbar.innerHTML = '<label>Results <select id="dqd-file" aria-label="Results file"></select></label>' +
        '<button id="dqd-reload" type="button">Reload</button>' +
        '<span id="dqd-load-state" role="status"></span>';
    document.querySelector('#overview .w-100').prepend(toolbar);
    const errorElement = document.createElement('div');
    errorElement.id = 'dqd-error';
    errorElement.setAttribute('role', 'alert');
    errorElement.hidden = true;
    toolbar.after(errorElement);
    const fileSelect = document.getElementById('dqd-file');
    let requestSequence = 0;
    const originalLoadResults = window.loadResults;

    // Shiny delivered one payload per session. Reload explicitly destroys the
    // prior DataTable before rendering the equivalent replacement payload.
    window.loadResults = function (results) {
        if ($.fn.dataTable.isDataTable('#dt-results')) {
            $('#dt-results tbody').off('click', 'td.details-control');
            $('#dt-results').DataTable().destroy();
            $('#dt-results').empty();
        }
        originalLoadResults(results);
    };

    function showError(error) {
        errorElement.textContent = error.message || String(error);
        errorElement.hidden = false;
    }
    async function getJson(url) {
        const response = await fetch(url, {cache: 'no-store'});
        const body = await response.json();
        if (!response.ok) throw new Error(body.error || ('HTTP ' + response.status));
        return body;
    }
    function resultsUrl(file) {
        return '/api/results' + (file ? '?file=' + encodeURIComponent(file) : '');
    }
    async function loadSelected() {
        const sequence = ++requestSequence;
        errorElement.hidden = true;
        document.getElementById('dqd-load-state').textContent = 'Loading';
        try {
            const results = await getJson(resultsUrl(fileSelect.value));
            if (sequence !== requestSequence) return;
            window.loadResults(results);
            document.getElementById('dqd-load-state').textContent = results.CheckResults.length + ' checks';
        } catch (error) {
            if (sequence === requestSequence) {
                document.getElementById('dqd-load-state').textContent = 'Load failed';
                showError(error);
            }
        }
    }
    fileSelect.addEventListener('change', loadSelected);
    document.getElementById('dqd-reload').addEventListener('click', loadSelected);
    getJson('/api/files').then(function (catalog) {
        catalog.files.forEach(function (file) { fileSelect.add(new Option(file, file)); });
        fileSelect.value = catalog.default || (catalog.files[0] || '');
        if (catalog.files.length === 0) throw new Error('No JSON result files exist in the configured results root.');
        return loadSelected();
    }).catch(showError);
})();
