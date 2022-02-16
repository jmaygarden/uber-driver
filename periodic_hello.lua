while true do
    noop()
    print "Hello, world!"
    sleep(5)

    local res, err = get_date()
    if res then
        print(res.stdout:match("(.-)%s*$"))
    else
        print("error", err)
    end
end
